use super::*;
use crate::core::geom::Size;
use crate::core::style::{Palette, RenderContext, TextRendering};
use crate::css::ColorScheme;
use crate::pipeline::page_load::{PageLoad, PageLoadOptions};
use encoding_rs::UTF_8;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

fn render_queue_contract(queue: Arc<dyn RenderQueue>) {
    let mut load = PageLoad::new(
        "<p>queued render</p>",
        Url::parse("https://example.com/").unwrap(),
        UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols: 40, rows: 12 }),
            palette: Palette::default(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    load.defer_rendering();
    assert!(load.render_if_ready(Duration::ZERO).is_none());
    let key = RenderKey {
        tab_id: 3,
        generation: 5,
        epoch: load.render_epoch(),
        hard_epoch: load.hard_epoch(),
    };
    assert!(matches!(
        queue.submit(load.take_render_job(key).unwrap()),
        RenderSubmitted::Queued
    ));
    let result = (0..100_000)
        .find_map(|_| match queue.poll() {
            RenderPoll::Ready(result) => Some(*result),
            RenderPoll::Empty => {
                std::thread::yield_now();
                None
            }
            RenderPoll::Disconnected => panic!("render queue disconnected"),
        })
        .expect("render queue did not return its bounded job");
    let page = load.apply_render_result(result).unwrap();
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line.contains("queued render"))
    );
    queue.shutdown();
}

#[test]
fn blocking_render_queue_passes_the_contract() {
    render_queue_contract(Arc::new(BlockingRenderQueue::default()));
}

#[test]
fn threaded_render_queue_passes_the_contract() {
    render_queue_contract(Arc::new(ThreadedRenderQueue::new()));
}

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
    assert_eq!(page.css_warnings, 1);
    assert!(
        page.styles.get(page.document.borrow().roots()[0]).display
            != crate::core::style::Display::NONE
    );
}

#[test]
fn declared_media_types_are_authoritative() {
    assert!(matches!(
        response_kind(Some("text/html; charset=utf-8"), b"plain"),
        ResponseKind::Html
    ));
    assert!(matches!(
        response_kind(Some("application/xhtml+xml"), b"plain"),
        ResponseKind::Html
    ));
    assert!(matches!(
        response_kind(Some("text/plain"), b"<html>not markup</html>"),
        ResponseKind::PlainText
    ));
    assert!(matches!(
        response_kind(Some("image/png"), b"<html>not an image</html>"),
        ResponseKind::Unsupported(kind) if kind == "image/png"
    ));
    assert!(matches!(
        response_kind(Some("application/octet-stream"), b"<html>not markup</html>"),
        ResponseKind::Unsupported(kind) if kind == "application/octet-stream"
    ));
}

#[test]
fn unknown_media_types_recognize_only_the_whatwg_html_signatures() {
    let signatures: &[&[u8]] = &[
        b"<!DOCTYPE HTML>",
        b"<HTML>",
        b"<HEAD>",
        b"<SCRIPT>",
        b"<IFRAME>",
        b"<H1>",
        b"<DIV>",
        b"<FONT>",
        b"<TABLE>",
        b"<A>",
        b"<STYLE>",
        b"<TITLE>",
        b"<B>",
        b"<BODY>",
        b"<BR>",
        b"<P>",
        b"<!-->",
    ];
    for signature in signatures {
        let mut body = b"\t\n\x0c\r ".to_vec();
        body.extend_from_slice(signature);
        assert!(matches!(response_kind(None, &body), ResponseKind::Html));
    }

    for content_type in [
        "not a media type",
        "unknown/unknown",
        "application/unknown",
        "*/*",
    ] {
        assert!(matches!(
            response_kind(Some(content_type), b"<p>html</p>"),
            ResponseKind::Html
        ));
    }

    for body in [b"<htmlx>plain".as_slice(), b"<html\t>plain", b"<svg>plain"] {
        assert!(matches!(response_kind(None, body), ResponseKind::PlainText));
    }
}

#[test]
fn unknown_media_types_distinguish_text_from_unsupported_content() {
    assert!(matches!(response_kind(None, b""), ResponseKind::PlainText));
    assert!(matches!(
        response_kind(None, b"ordinary text"),
        ResponseKind::PlainText
    ));
    assert!(matches!(
        response_kind(None, b"\xef\xbb\xbftext\0"),
        ResponseKind::PlainText
    ));
    assert!(matches!(
        response_kind(None, b"\xff\xfeT\0"),
        ResponseKind::PlainText
    ));
    assert!(matches!(
        response_kind(None, b" \n<?xml version='1.0'?>"),
        ResponseKind::Unsupported(kind) if kind == "text/xml"
    ));
    assert!(matches!(
        response_kind(None, b"%PDF-1.7"),
        ResponseKind::Unsupported(kind) if kind == "application/pdf"
    ));
    assert!(matches!(
        response_kind(None, b"%!PS-Adobe-3.0"),
        ResponseKind::Unsupported(kind) if kind == "application/postscript"
    ));
    assert!(matches!(
        response_kind(None, b"text\0binary"),
        ResponseKind::Unsupported(kind) if kind == "application/octet-stream"
    ));
}

#[test]
fn sniffing_reads_only_the_whatwg_resource_header() {
    let mut outside_header = vec![b'a'; 1_445];
    outside_header.push(0);
    assert!(matches!(
        response_kind(None, &outside_header),
        ResponseKind::PlainText
    ));

    let mut inside_header = vec![b'a'; 1_445];
    inside_header[1_444] = 0;
    assert!(matches!(
        response_kind(None, &inside_header),
        ResponseKind::Unsupported(kind) if kind == "application/octet-stream"
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

#[test]
fn negative_inline_margin_overlaps_and_later_content_paints_on_top() {
    let page = render_html(
        "<body style='margin:0'>A<span style='margin-left:-1ch'>B</span></body>",
        Size { cols: 8, rows: 4 },
        Palette::default(),
        false,
    );
    assert_eq!(page.painted.text_lines()[0].trim_end(), "B");
}

#[test]
fn mixed_css_math_reaches_parent_relative_layout() {
    let page = render_html(
        "<body style='margin:0'><div style='width:calc(50% - 1ch)'>X</div></body>",
        Size { cols: 10, rows: 4 },
        Palette::default(),
        false,
    );
    assert!(page.painted.hits.iter().any(|hit| hit.rect.width == 4));
}

#[test]
fn comparison_math_resolves_after_the_containing_width_is_known() {
    for (value, cols, expected) in [
        ("min(75%, 6ch)", 4, 3),
        ("min(75%, 6ch)", 12, 6),
        ("max(50%, 6ch)", 8, 6),
        ("max(50%, 6ch)", 16, 8),
        ("clamp(3ch, 75%, 6ch)", 4, 3),
        ("clamp(3ch, 75%, 6ch)", 8, 6),
        ("clamp(8ch, 50%, 4ch)", 10, 8),
        ("clamp(none, 75%, 6ch)", 4, 3),
        ("clamp(3ch, 25%, none)", 16, 4),
        ("clamp(none, 50%, none)", 10, 5),
        ("calc(min(75%, 6ch) + 1ch)", 4, 4),
        ("calc(min(75%, 6ch) + 1ch)", 12, 7),
    ] {
        let page = render_html(
            &format!("<body style='margin:0'><div style='width:{value}'>X</div></body>"),
            Size { cols, rows: 4 },
            Palette::default(),
            false,
        );
        assert!(
            page.painted
                .hits
                .iter()
                .any(|hit| hit.rect.width == expected),
            "{value} at a {cols}-cell basis did not resolve to {expected} cells"
        );
    }
}

#[test]
fn comparison_math_survives_custom_property_substitution() {
    let page = render_html(
        "<body style='margin:0'><div style='--measure:min(75%, 6ch);width:var(--measure)'>X</div></body>",
        Size { cols: 8, rows: 4 },
        Palette::default(),
        false,
    );
    assert!(page.painted.hits.iter().any(|hit| hit.rect.width == 6));
}
