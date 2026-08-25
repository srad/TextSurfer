use std::path::Path;
use std::time::Duration;

use textsurfer::core::geom::Size;
use textsurfer::core::style::{
    BorderCollapse, BorderSpacing, CaptionSide, Palette, Rgb, Rgba, TextRendering,
};
use textsurfer::css::{ColorScheme, DynamicState};
use textsurfer::paint::DisplayList;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::pipeline::render::{RenderedPage, render_html};

const WIDTH: usize = 40;

fn palette() -> Palette {
    Palette {
        text: Rgb::new(255, 255, 255),
        background: Rgb::new(0, 0, 128),
        link: Rgb::new(255, 255, 0),
        link_hover: Rgb::new(0, 255, 255),
    }
}

fn render(name: &str) -> DisplayList {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {}: {error}", path.display()));
    render_html(
        &source,
        textsurfer::core::geom::Size {
            cols: WIDTH as u16,
            rows: 24,
        },
        palette(),
        false,
    )
    .painted
}

#[test]
fn stateful_render_reuses_one_page_load_and_restores_hover_style() {
    let mut load = PageLoad::new(
        "<!doctype html><style>a:hover { color: red }</style><a href=/>target</a>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            viewport: Size { cols: 40, rows: 24 },
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
            text_rendering: TextRendering::Cell,
        },
    );
    let first = load.force_render();
    let link = first.painted.links[0].node;
    assert_eq!(first.styles.get(link).color, Some(palette().link.into()));
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(link),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hovered.painted.links[0].node, link);
    assert_eq!(
        hovered.styles.get(link).color,
        Some(Rgb::new(255, 0, 0).into())
    );
    let restored = load.set_dynamic_state(DynamicState::INERT).unwrap();
    assert_eq!(restored.styles.get(link).color, Some(palette().link.into()));
}

#[test]
fn dynamic_flex_restyle_reflows_and_restores_the_public_render_path() {
    let mut load = PageLoad::new(
        "<!doctype html><style>body{margin:0}main{display:flex;width:4ch}main:hover{flex-direction:column}span{width:2ch;flex:none}</style><main><span>A</span><span>B</span></main>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            viewport: Size { cols: 20, rows: 8 },
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
            text_rendering: TextRendering::Cell,
        },
    );
    let first = load.force_render();
    assert_eq!(nonempty_lines(&first), ["A B"]);
    let target = first.painted.hit_test(0, 0).unwrap();
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(target),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(nonempty_lines(&hovered), ["A", "B"]);
    let restored = load.set_dynamic_state(DynamicState::INERT).unwrap();
    assert_eq!(nonempty_lines(&restored), ["A B"]);
}

fn golden(name: &str) -> String {
    render(name).text_lines().join("\n")
}

fn render_source(source: &str, width: u16) -> RenderedPage {
    render_html(
        source,
        textsurfer::core::geom::Size {
            cols: width,
            rows: 24,
        },
        palette(),
        false,
    )
}

fn nonempty_lines(page: &RenderedPage) -> Vec<String> {
    page.painted
        .text_lines()
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn responsive_css_pixel_breakpoint_uses_the_terminal_cell_metric() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("responsive_units.html");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {}: {error}", path.display()));
    assert_eq!(nonempty_lines(&render_source(&source, 79)), ["NARROW"]);
    assert_eq!(nonempty_lines(&render_source(&source, 80)), ["WIDE"]);
}

#[test]
fn margins_and_padding_golden() {
    insta::assert_snapshot!(golden("margins.html"));
}

#[test]
fn headings_lists_and_rules_golden() {
    insta::assert_snapshot!(golden("headings.html"));
}

#[test]
fn list_markers_and_counters_golden() {
    insta::assert_snapshot!(golden("lists.html"));
}

#[test]
fn generated_content_golden() {
    insta::assert_snapshot!(golden("generated.html"));
}

#[test]
fn borders_golden() {
    insta::assert_snapshot!(golden("borders.html"));
}

#[test]
fn links_golden() {
    insta::assert_snapshot!(golden("links.html"));
}

#[test]
fn wide_characters_golden() {
    insta::assert_snapshot!(golden("wide.html"));
}

#[test]
fn preformatted_golden() {
    insta::assert_snapshot!(golden("pre.html"));
}

#[test]
fn simple_table_golden() {
    insta::assert_snapshot!(golden("table_simple.html"));
}

#[test]
fn collapsed_spanned_table_golden() {
    insta::assert_snapshot!(golden("table_collapsed.html"));
}

#[test]
fn nested_captioned_tables_golden() {
    insta::assert_snapshot!(golden("table_nested.html"));
}

#[test]
fn fixed_table_overflow_golden() {
    insta::assert_snapshot!(golden("table_fixed_overflow.html"));
}

#[test]
fn outer_and_inner_display_modes_golden() {
    insta::assert_snapshot!(golden("display_modes.html"));
}

#[test]
fn flex_layout_golden() {
    insta::assert_snapshot!(golden("flex.html"));
}

#[test]
fn custom_properties_golden() {
    insta::assert_snapshot!(golden("variables.html"));
}

#[test]
fn custom_properties_match_literal_style_geometry_and_text() {
    let variable = render_source(
        "<style>body{margin:0;--w:12ch;--c:#00ff00}#card{display:block;width:var(--w);color:var(--c)}</style><div id=card>Alpha Beta Gamma</div>",
        30,
    );
    let literal = render_source(
        "<style>body{margin:0}#card{display:block;width:12ch;color:#00ff00}</style><div id=card>Alpha Beta Gamma</div>",
        30,
    );
    assert_eq!(variable.painted.text_lines(), literal.painted.text_lines());
    let variable_node = variable.document.borrow().element_by_id("card").unwrap();
    let literal_node = literal.document.borrow().element_by_id("card").unwrap();
    assert_eq!(
        variable.styles.get(variable_node),
        literal.styles.get(literal_node)
    );
    assert_eq!(
        variable
            .painted
            .hits
            .iter()
            .map(|hit| hit.rect)
            .collect::<Vec<_>>(),
        literal
            .painted
            .hits
            .iter()
            .map(|hit| hit.rect)
            .collect::<Vec<_>>()
    );
}

#[test]
fn dynamic_custom_property_restyle_reuses_the_public_page_load() {
    let mut load = PageLoad::new(
        "<!doctype html><style>a{--tone:#ffff00;color:var(--tone)}a:hover{--tone:#ff0000}</style><a href=/>target</a>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            viewport: Size { cols: 20, rows: 8 },
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
            text_rendering: TextRendering::Cell,
        },
    );
    let first = load.force_render();
    let link = first.painted.links[0].node;
    assert_eq!(
        first.styles.get(link).color,
        Some(Rgba::new(255, 255, 0, 255))
    );
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(link),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        hovered.styles.get(link).color,
        Some(Rgba::new(255, 0, 0, 255))
    );
}

#[test]
fn flex_visual_order_and_dom_link_order_survive_the_public_render_path() {
    let page = render_source(
        "<style>body{margin:0}main{display:flex;width:12ch}a{flex:none;width:6ch}a:first-child{order:2}a:last-child{order:-1}</style><main><a href=first>FIRST</a><a href=second>SECOND</a></main>",
        20,
    );
    assert_eq!(page.painted.links[0].href, "first");
    assert_eq!(page.painted.links[1].href, "second");
    assert_eq!(page.painted.links[0].rects[0].col, 6);
    assert_eq!(page.painted.links[1].rects[0].col, 0);
    assert_eq!(
        page.painted.link_at(1, 0).map(|link| link.href.as_str()),
        Some("second")
    );
    assert_eq!(
        page.painted.link_at(7, 0).map(|link| link.href.as_str()),
        Some("first")
    );
}

#[test]
fn presentational_html_golden() {
    insta::assert_snapshot!(golden("presentational.html"));
}

#[test]
fn text_alignment_moves_fragments_and_link_geometry_together() {
    let page = render_source(
        "<style>p { margin:0 }</style><p style='text-align:right'><a href=x>R</a></p>
         <p style='text-align:center'>C</p><p style='text-align:justify'>J</p>",
        20,
    );
    let right = page.painted.links.first().expect("right-aligned link");
    assert_eq!(right.rects[0].col, 19);
    let rows = page.painted.text_lines();
    assert_eq!(rows[1].chars().position(|ch| ch == 'C'), Some(10));
    assert_eq!(rows[2].chars().position(|ch| ch == 'J'), Some(0));
}

#[test]
fn table_cell_vertical_alignment_uses_the_final_row_height() {
    let page = render_source(
        "<style>table{border-spacing:0;margin:0}td{padding:0;width:5ch}</style>
         <table><tr><td valign=bottom>B</td><td>T<br>T</td><td valign=middle>M</td></tr></table>",
        20,
    );
    let lines = page.painted.text_lines();
    let b_row = lines.iter().position(|line| line.contains('B')).unwrap();
    let m_row = lines.iter().position(|line| line.contains('M')).unwrap();
    assert_eq!(b_row, 1);
    assert_eq!(m_row, 1);
}

#[test]
fn center_aligns_an_intrinsic_table_descendant() {
    let page = render_source(
        "<center><table style='border-spacing:0;width:4ch;margin:0'><tr><td style='padding:0;text-align:left'>X</td></tr></table></center>",
        20,
    );
    let line = &page.painted.text_lines()[0];
    assert_eq!(line.chars().position(|ch| ch == 'X'), Some(8));
}

#[test]
fn table_cells_share_normal_word_and_white_space_behavior() {
    let wrapping = render_source(
        "<style>table { border-spacing: 0; table-layout: fixed; width: 6ch; margin: 0 }
         td { padding: 0 }</style><table><tr><td>alpha beta</td></tr></table>",
        20,
    );
    assert_eq!(nonempty_lines(&wrapping), ["alpha", "beta"]);

    let accented = render_source(
        "<style>table { border-spacing: 0; table-layout: fixed; width: 6ch; margin: 0 }
         td { padding: 0 }</style><table><tr><td>Grüße Grüße</td></tr></table>",
        20,
    );
    assert_eq!(nonempty_lines(&accented), ["Grüße", "Grüße"]);

    let wide = render_source(
        "<style>table { border-spacing: 0; table-layout: fixed; width: 6ch; margin: 0 }
         td { padding: 0 }</style><table><tr><td>日本語のテキスト</td></tr></table>",
        20,
    );
    let wide_lines = nonempty_lines(&wide);
    assert!(wide_lines.len() > 1);
    assert_eq!(wide_lines.concat(), "日本語のテキスト");

    let nowrap = render_source(
        "<style>table { border-spacing: 0; table-layout: fixed; width: 20ch; margin: 0 }
         td { padding: 0; white-space: nowrap }</style><table><tr><td>left
         right<br>end</td></tr></table>",
        24,
    );
    assert_eq!(nonempty_lines(&nowrap), ["left right", "end"]);
}

#[test]
fn nested_tables_preserve_source_order_and_inline_outer_display() {
    let page = render_source(
        "<style>table { border-spacing: 0; margin: 0 } td { padding: 0 }
         .inline { display: inline-table }
         .inline > span { display: table-row }
         .inline > span > span { display: table-cell }</style>
         <table><tr><td>before<table><tr><td>block</td></tr></table>after</td></tr>
         <tr><td>Before <span class=inline><span><span>Inline</span></span></span> after</td></tr></table>",
        40,
    );
    let lines = nonempty_lines(&page);
    let before = lines.iter().position(|line| line == "before").unwrap();
    let block = lines.iter().position(|line| line == "block").unwrap();
    let after = lines.iter().position(|line| line == "after").unwrap();
    assert!(before < block && block < after);
    assert!(lines.iter().any(|line| line == "Before Inline after"));
}

#[test]
fn captions_apply_box_geometry_paint_and_hit_regions() {
    let page = render_source(
        "<style>table { border-spacing: 0; margin: 0 } td { padding: 0 }
         caption { border: solid; padding: 1rem 1ch; background: #008000 }
         .bottom { caption-side: bottom }</style>
         <table><caption id=top>Top</caption><tr><td>x</td></tr>
         <caption id=bottom class=bottom>Bottom</caption></table>",
        30,
    );
    let document = page.document.borrow();
    let top = document.element_by_id("top").unwrap();
    let bottom = document.element_by_id("bottom").unwrap();
    let top_hit = page
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == top)
        .unwrap();
    let bottom_hit = page
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == bottom)
        .unwrap();
    assert_eq!(top_hit.rect.height, 5);
    assert_eq!(bottom_hit.rect.height, 5);
    assert!(top_hit.rect.row < bottom_hit.rect.row);
    assert_eq!(
        page.painted
            .hit_test(top_hit.rect.col + 1, top_hit.rect.row + 1),
        Some(top)
    );
    assert!(
        page.painted.rows[top_hit.rect.row + 1]
            .spans
            .iter()
            .any(|span| span.style.bg == Some(Rgb::new(0, 128, 0)))
    );
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line.contains('┌'))
    );

    let empty = render_source(
        "<style>table { border-spacing: 0; margin: 0 } caption { border: solid; padding: 1rem 1ch }
         td { padding: 0 }</style><table><caption id=empty></caption><tr><td>x</td></tr></table>",
        20,
    );
    let empty_node = empty.document.borrow().element_by_id("empty").unwrap();
    let empty_hit = empty
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == empty_node)
        .unwrap();
    assert_eq!(empty_hit.rect.height, 4);
}

#[test]
fn anonymous_table_fixup_groups_only_consecutive_non_cells() {
    let page = render_source(
        "<div style='display:table;border-spacing:1ch 0'>
           <span style='display:table-cell'>A</span><span>X</span><span>Y</span><span style='display:table-cell'>B</span>
         </div>",
        30,
    );
    assert!(nonempty_lines(&page).iter().any(|line| line == "A XY B"));

    let interrupted = render_source(
        "<div style='display:table;border-spacing:0'>
           <span>X</span><span style='display:table-caption'>Caption</span><span>Y</span>
         </div>",
        30,
    );
    let lines = nonempty_lines(&interrupted);
    assert!(lines.iter().any(|line| line == "X"));
    assert!(lines.iter().any(|line| line == "Y"));
}

#[test]
fn column_group_widths_contribute_in_auto_and_fixed_layout() {
    fn separation(source: &str) -> usize {
        let page = render_source(source, 30);
        page.painted
            .text_lines()
            .into_iter()
            .find_map(|line| {
                let a = line.chars().position(|ch| ch == 'A')?;
                let b = line.chars().position(|ch| ch == 'B')?;
                Some(b - a)
            })
            .unwrap()
    }

    let auto = separation(
        "<style>table { border-spacing: 0; margin: 0 } td { padding: 0 }
         colgroup { width: 6ch }</style>
         <table><colgroup><col><col></colgroup><tr><td>A</td><td>B</td></tr></table>",
    );
    let fixed = separation(
        "<style>table { border-spacing: 0; table-layout: fixed; width: 4ch; margin: 0 }
         td { padding: 0 } colgroup { width: 6ch }</style>
         <table><colgroup><col><col></colgroup><tr><td>A</td><td>B</td></tr></table>",
    );
    assert!(auto >= 6);
    assert!(fixed >= 6);
}

#[test]
fn table_properties_inherit_through_the_public_render_path() {
    let page = render_source(
        "<div style='border-collapse:collapse;border-spacing:3ch 2rem;caption-side:bottom'>
           <div id=table style='display:table'>
             <span id=caption style='display:table-caption'>Caption</span>
             <span style='display:table-row'><span style='display:table-cell'>Cell</span></span>
           </div>
         </div>",
        30,
    );
    let document = page.document.borrow();
    let table = document.element_by_id("table").unwrap();
    let caption = document.element_by_id("caption").unwrap();
    assert_eq!(
        page.styles.get(table).border_collapse,
        BorderCollapse::Collapse
    );
    assert_eq!(
        page.styles.get(table).border_spacing,
        BorderSpacing::new(3, 2)
    );
    assert_eq!(page.styles.get(caption).caption_side, CaptionSide::Bottom);
}

#[test]
fn clipped_nested_tables_do_not_relocate_their_far_border() {
    let page = render_source(
        "<style>table { border-spacing: 0; margin: 0 } td { padding: 0; border: none }
         .outer { table-layout: fixed; width: 8ch }
         .inner { width: 20ch; border: solid }</style>
         <table class=outer><tr><td><table class=inner><tr><td>x</td></tr></table></td></tr></table>",
        30,
    );
    let top = page
        .painted
        .text_lines()
        .into_iter()
        .find(|line| line.contains('┌'))
        .unwrap();
    assert!(!top.ends_with('┐'), "{top:?}");
}

#[test]
fn painted_rows_never_exceed_the_render_width() {
    for fixture in [
        "margins.html",
        "headings.html",
        "borders.html",
        "links.html",
        "wide.html",
        "pre.html",
        "table_simple.html",
        "table_collapsed.html",
        "table_nested.html",
        "table_fixed_overflow.html",
    ] {
        for line in render(fixture).text_lines() {
            assert!(
                unicode_width::UnicodeWidthStr::width(line.as_str()) <= WIDTH,
                "{fixture} painted past the viewport: {line:?}"
            );
        }
    }
}

#[test]
fn link_styling_and_geometry_survive_into_the_display_list() {
    let painted = render("links.html");
    assert_eq!(
        painted.links.len(),
        2,
        "only anchors with href are interactive"
    );
    let first = &painted.links[0];
    assert_eq!(first.href, "https://example.com/");
    assert!(!first.rects.is_empty(), "links must carry their geometry");
    let rect = first.rects[0];
    assert_eq!(
        painted.link_at(rect.col, rect.row).map(|link| &link.href),
        Some(&first.href)
    );

    let accent = palette().link;
    let styled = painted.rows[rect.row]
        .spans
        .iter()
        .find(|span| span.col == rect.col)
        .expect("a span starts where the link starts");
    assert_eq!(
        styled.style.fg,
        Some(accent.into()),
        "UA link colour is painted"
    );
    assert!(styled.style.underline);
}

#[test]
fn dump_mode_prints_the_same_page_the_painter_produced() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/headings.html");
    let url = url::Url::from_file_path(&path).expect("fixture path is absolute");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_textsurfer"))
        .args(["--dump", "--cols", "40", "--url", url.as_str()])
        .output()
        .expect("dump run");
    assert!(
        output.status.success(),
        "dump failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("utf-8 dump");
    let expected = golden("headings.html");
    assert_eq!(
        printed.replace("\r\n", "\n").trim_end(),
        expected.trim_end(),
        "--dump must agree with the in-process painter"
    );
}

#[test]
fn unreadable_author_colours_are_corrected_before_painting() {
    let painted = render("links.html");
    let dark = painted
        .rows
        .iter()
        .flat_map(|row| row.spans.iter())
        .find(|span| span.text.contains("nearly black"))
        .expect("the dark paragraph is painted");
    let foreground = dark.style.fg.expect("author colour applied");
    assert!(
        foreground.rgb.contrast_ratio(palette().background) >= 3.0,
        "author colour {foreground:?} must be corrected against the field"
    );
}

#[test]
fn foreground_alpha_survives_the_complete_render_path() {
    let painted = render_html(
        "<div style='background:#000080'><p style='margin:0;background:#000'>
           <span style='color:rgba(255,255,255,0.5)'>half</span><a href='https://example.com/'
           style='color:transparent'>secret</a><span>after</span></p></div>",
        textsurfer::core::geom::Size { cols: 30, rows: 4 },
        palette(),
        false,
    )
    .painted;
    let line = painted.text_lines().join("\n");
    assert!(line.contains("half"));
    assert!(line.contains("after"));
    assert!(!line.contains("secret"));
    let half = painted
        .rows
        .iter()
        .flat_map(|row| row.spans.iter())
        .find(|span| span.text == "half")
        .expect("partial foreground reaches the display list");
    assert_eq!(half.style.bg, Some(Rgb::BLACK));
    assert_eq!(half.style.fg, Some(Rgba::new(128, 128, 128, 255)));
    let link = painted.links.first().expect("the transparent link remains");
    let rect = link
        .rects
        .first()
        .expect("the transparent link has geometry");
    assert_eq!(rect.width, 6);
    assert_eq!(painted.link_at(rect.col + 2, rect.row), Some(link));
}

/// `background` is not an inherited property, yet a pseudo box starts from the originating
/// element's computed style, background included. That cannot show: generated content is
/// inline-level and the outside marker hangs in a field reserved inside the item's own box, so a
/// pseudo box only ever paints over the background its originating element already painted. This
/// pins that — including the adversarial case where the pseudo declares `background: initial`,
/// which is transparent in CSS and must reveal the item's background rather than the theme field.
#[test]
fn pseudo_boxes_never_own_a_background_their_element_did_not_paint() {
    let page = render_source(
        "<style>ul { margin: 0; padding: 0; background: #000080 }
         li { margin: 0; background: #008000; list-style-type: decimal }
         li::before { content: 'B'; background: initial } li::after { content: 'A' }
         li::marker { background: initial }</style>
         <ul><li id=item>text</li></ul>",
        20,
    );
    let row = page.painted.row(0).expect("the item paints one row");
    let line: String = row.spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(line.trim_end(), "1. BtextA");
    for span in &row.spans {
        assert_eq!(
            span.style.bg,
            Some(Rgb::new(0, 128, 0)),
            "column {} escaped the item background",
            span.col
        );
    }
    // The marker field and both generated pieces still answer as the originating element.
    let item = page.document.borrow().element_by_id("item").unwrap();
    for col in [0, 3, 8] {
        assert_eq!(page.painted.hit_test(col, 0), Some(item));
    }
}
