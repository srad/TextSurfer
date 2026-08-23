use super::*;
use crate::core::geom::Size;
use crate::core::style::Palette;

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
            != crate::core::style::Display::None
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
