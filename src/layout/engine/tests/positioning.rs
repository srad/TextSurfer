use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::core::style::Palette;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};

fn render(source: &str, cols: u16) -> crate::pipeline::render::RenderedPage {
    crate::pipeline::render::render_html(source, Size { cols, rows: 24 }, Palette::DEFAULT, false)
}

#[test]
fn relative_insets_move_a_box_without_removing_its_flow_space() {
    let page = render(
        "<div style='position:relative;left:16px;width:24px'>x</div><p>after</p>",
        20,
    );
    let rows = page.painted.text_lines();
    assert!(rows[0].starts_with("  x"));
    assert!(rows.iter().skip(1).any(|row| row.contains("after")));
}

#[test]
fn absolute_descendants_resolve_against_the_nearest_positioned_ancestor() {
    let page = render(
        "<div style='position:relative;width:80px;height:64px'><div><span style='position:absolute;left:16px;top:16px'>x</span></div></div>",
        20,
    );
    let rows = page.painted.text_lines();
    assert!(rows[1].starts_with("  x"));
}

#[test]
fn fixed_subtrees_do_not_extend_document_height() {
    let mut document = Document::new();
    let fixed = document.insert_element(None, "div", ElementNs::Html, vec![]);
    document.insert_text(Some(fixed), "fixed");
    let normal = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(normal), "normal");
    let sheet = CssparserParser.parse("div { position:fixed;top:160px }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    assert!(
        tree.fragments
            .iter()
            .any(|fragment| fragment.text == "fixed")
    );
    assert!(tree.height < 10);
}

#[test]
fn negative_absolute_origins_clip_leading_glyphs_instead_of_translating_the_run() {
    let page = render(
        "<span style='position:absolute;left:-16px;top:0;width:24px;white-space:nowrap'>abc</span>",
        20,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains("abc"));
    assert!(text.contains('c'), "{text:?}");
}

#[test]
fn negative_table_origins_clip_the_table_atom_instead_of_translating_it() {
    let page = render(
        "<table style='position:absolute;left:-16px;top:0;width:40px;table-layout:fixed;border-collapse:collapse'><tr><td style='white-space:nowrap'>abc</td></tr></table>",
        20,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains("abc"));
    assert!(text.contains('c'), "{text:?}");
}

#[test]
fn negative_inline_flex_origins_clip_nested_layout_instead_of_translating_it() {
    let page = render(
        "<span style='display:inline-flex;position:absolute;left:-16px;top:0;width:24px;white-space:nowrap'><span>a</span><span>b</span><span>c</span></span>",
        20,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains("abc"));
    assert!(text.contains('c'));
}
