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
