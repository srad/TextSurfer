use crate::core::geom::Size;
use crate::core::style::Palette;

fn render(source: &str, cols: u16) -> crate::pipeline::render::RenderedPage {
    crate::pipeline::render::render_html(source, Size { cols, rows: 24 }, Palette::DEFAULT, false)
}

#[test]
fn zero_height_hidden_overflow_emits_nothing_below_its_padding_box() {
    let page = render(
        "<div style='height:0;overflow:hidden'>leak</div><p>after</p>",
        20,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains("leak"));
    assert!(text.contains("after"));
}

/// Horizontal overflow, so a clipped glyph disappears instead of being overwritten by whatever the
/// next block paints on the same row — the confound a zero-height box would introduce.
fn narrow_box(containment: &str) -> String {
    let page = render(
        &format!(
            "<div style='width:16px;height:16px;white-space:nowrap;{containment}'>abcdefgh</div>"
        ),
        20,
    );
    page.painted.text_lines().join("\n")
}

#[test]
fn without_containment_a_narrow_box_still_paints_what_overflows_it() {
    assert!(narrow_box("").contains("abcdefgh"));
}

#[test]
fn paint_containment_clips_what_overflows_its_padding_box() {
    let text = narrow_box("contain:paint");
    assert!(text.contains("ab"));
    assert!(!text.contains("abc"));
}

#[test]
fn contain_shorthands_that_include_paint_clip_too() {
    for value in ["content", "strict", "layout paint"] {
        let text = narrow_box(&format!("contain:{value}"));
        assert!(!text.contains("abc"), "contain:{value} did not clip");
    }
}

#[test]
fn containment_without_paint_leaves_overflow_visible() {
    assert!(narrow_box("contain:layout").contains("abcdefgh"));
}

#[test]
fn horizontal_overflow_clips_whole_graphemes_without_reflowing_them() {
    let page = render(
        "<div style='width:8px;height:16px;overflow:hidden;white-space:nowrap'>a界b</div>",
        20,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(text.contains('a'));
    assert!(!text.contains('界'));
    assert!(!text.contains('b'));
}

#[test]
fn hidden_geometry_stays_in_flow_but_only_visible_descendants_paint_and_hit() {
    let page = render(
        "<a href='/hidden' style='visibility:hidden'>gone<span style='visibility:visible'>shown</span></a><p>after</p>",
        30,
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains("gone"));
    assert!(text.contains("shown"));
    assert!(text.contains("after"));
    assert_eq!(page.painted.links.len(), 1);
    assert_eq!(page.painted.links[0].rects.len(), 1);
}
