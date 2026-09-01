use std::path::Path;
use std::time::Duration;

use textsurfer::core::geom::Size;
use textsurfer::core::image::DecodedImage;
use textsurfer::core::style::{
    BorderCollapse, BorderSpacing, CaptionSide, Palette, RenderContext, Rgb, Rgba,
};
use textsurfer::css::{ColorScheme, DynamicState};
use textsurfer::net::FetchResponse;
use textsurfer::paint::{DisplayList, legible_foreground};
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
            render: RenderContext::terminal(Size { cols: 40, rows: 24 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
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
fn retained_row_background_restyle_updates_every_contributed_cell_layer() {
    let mut load = PageLoad::new(
        "<!doctype html><style>body{margin:0}table{border-collapse:collapse}tr{background:#004000}tr:hover{background:#400000}td{border:solid;padding:0 1ch}</style><table><tr><td>A</td><td>B</td></tr></table>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols: 20, rows: 8 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    let first = load.force_render();
    let a = first
        .painted
        .rows
        .iter()
        .flat_map(|row| &row.spans)
        .find(|span| span.text.contains('A'))
        .unwrap();
    assert_eq!(a.style.bg, Some(Rgb::new(0, 64, 0)));
    let target = first.painted.hit_test(a.col, 1).unwrap();
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(target),
            ..Default::default()
        })
        .unwrap();
    for label in ['A', 'B'] {
        let span = hovered
            .painted
            .rows
            .iter()
            .flat_map(|row| &row.spans)
            .find(|span| span.text.contains(label))
            .unwrap();
        assert_eq!(span.style.bg, Some(Rgb::new(64, 0, 0)));
    }
}

#[test]
fn retained_table_background_restyle_updates_collapsed_border_cells() {
    let mut load = PageLoad::new(
        "<!doctype html><style>body{margin:0}table{border-collapse:collapse;background:#202020}table:hover{background:#400000}tr{background:#004000}td{border:solid;padding:0 1ch}</style><table><tr><td>A</td><td>B</td></tr></table>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols: 20, rows: 8 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    let first = load.force_render();
    let (row, shared_col) = collapsed_shared_border(&first.painted);
    assert_eq!(
        background_at(&first.painted, row, shared_col),
        Some(Rgb::new(32, 32, 32))
    );
    let target = first
        .painted
        .hit_test(shared_col.saturating_sub(1), row)
        .unwrap();
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(target),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        background_at(&hovered.painted, row, shared_col),
        Some(Rgb::new(64, 0, 0))
    );
}

#[test]
fn collapsed_border_ink_topology_changes_rebuild_background_geometry() {
    let mut load = PageLoad::new(
        "<!doctype html><style>body{margin:0}table{border-collapse:collapse;background:#202020}tr{background:#004000}tr:hover td{border-color:transparent}td{border:solid #000;padding:0 1ch}</style><table><tr><td>A</td><td>B</td></tr></table>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols: 20, rows: 8 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    let first = load.force_render();
    let (row, shared_col) = collapsed_shared_border(&first.painted);
    assert_eq!(
        background_at(&first.painted, row, shared_col),
        Some(Rgb::new(32, 32, 32))
    );
    let target = first
        .painted
        .hit_test(shared_col.saturating_sub(1), row)
        .unwrap();
    let hovered = load
        .set_dynamic_state(DynamicState {
            hover: Some(target),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        background_at(&hovered.painted, row, shared_col),
        Some(Rgb::new(0, 64, 0))
    );
    let restored = load.set_dynamic_state(DynamicState::INERT).unwrap();
    assert_eq!(
        background_at(&restored.painted, row, shared_col),
        Some(Rgb::new(32, 32, 32))
    );
}

fn collapsed_shared_border(display: &DisplayList) -> (usize, usize) {
    display
        .rows
        .iter()
        .enumerate()
        .find_map(|(row, painted)| {
            painted
                .spans
                .iter()
                .find(|span| span.text.contains('│') && span.col > 0)
                .map(|span| (row, span.col))
        })
        .unwrap()
}

fn background_at(display: &DisplayList, row: usize, col: usize) -> Option<Rgb> {
    display.rows[row]
        .spans
        .iter()
        .find(|span| span.col <= col && col < span.col + span.text.chars().count())
        .and_then(|span| span.style.bg)
}

#[test]
fn dynamic_flex_restyle_reflows_and_restores_the_public_render_path() {
    let mut load = PageLoad::new(
        "<!doctype html><style>body{margin:0}main{display:flex;width:4ch}main:hover{flex-direction:column}span{width:2ch;flex:none}</style><main><span>A</span><span>B</span></main>",
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols: 20, rows: 8 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
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

fn load_source_with_image(source: &str, width: u16) -> PageLoad {
    let mut load = PageLoad::new(
        source,
        url::Url::parse("https://example.com/").unwrap(),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size {
                cols: width,
                rows: 24,
            }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    load.force_render();
    let command = load.take_commands().pop().unwrap();
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
        Ok(DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision,
            width: 64,
            height: 64,
            rgba: vec![255; 64 * 64 * 4].into(),
            source: textsurfer::core::image::DecodedImageSource::Raster,
        })
    ));
    load
}

fn render_source_with_image(source: &str, width: u16) -> RenderedPage {
    let mut load = load_source_with_image(source, width);
    load.render_after_image().unwrap()
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
fn leading_floats_shape_each_line_and_clear_restores_the_full_measure() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}aside{float:left;width:4ch;height:2rem}</style><main><aside>LLLL<br>LLLL</aside>alpha beta gamma delta<br clear=all>clear line</main>",
        12,
    );
    assert_eq!(
        page.painted.text_lines(),
        ["LLLLalpha", "LLLLbeta", "gamma delta", "clear line"]
    );
}

#[test]
fn a_float_after_wrapped_text_starts_on_the_current_source_line() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}.float{float:left;width:3ch;height:2rem}</style><main>one two three four <span class=float>FFF<br>FFF</span> tail</main>",
        12,
    );
    assert_eq!(
        page.painted.text_lines(),
        ["one two", "FFFthree", "FFFfour tail"]
    );
}

#[test]
fn opposing_floats_leave_a_safe_middle_band_and_keep_links_clickable() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}.l{float:left;width:3ch}.r{float:right;width:3ch}</style><main><a class=l href=/left>LLL</a><a class=r href=/right>RRR</a>middle text</main>",
        12,
    );
    assert_eq!(page.painted.text_lines(), ["LLLmiddleRRR", "text"]);
    assert_eq!(page.painted.links.len(), 2);
    assert_eq!(
        page.painted.link_at(0, 0).map(|link| link.node),
        Some(page.painted.links[0].node)
    );
    assert_eq!(
        page.painted.link_at(11, 0).map(|link| link.node),
        Some(page.painted.links[1].node)
    );
}

#[test]
fn an_empty_generated_clearfix_contains_the_float_for_following_content() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}.group::after{content:'';display:block;clear:both}.float{float:left;width:4ch;height:2rem}</style><div class=group><span class=float>FFFF<br>FFFF</span>inside</div><p>after</p>",
        12,
    );
    assert_eq!(page.painted.text_lines(), ["FFFFinside", "FFFF", "after"]);
}

#[test]
fn float_decoration_paints_above_a_later_in_flow_block_background() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}.float{float:left;width:4ch;height:2rem;background:red}.normal{height:2rem;background:blue}</style><main><span class=float></span><div class=normal></div></main>",
        12,
    );
    let background_at = |col: usize, row: usize| {
        page.painted.rows[row]
            .spans
            .iter()
            .find(|span| {
                span.col <= col && col < span.col.saturating_add(span.text.chars().count())
            })
            .and_then(|span| span.style.bg)
    };
    assert_eq!(background_at(1, 1), Some(Rgb::new(255, 0, 0)));
    assert_eq!(background_at(5, 1), Some(Rgb::new(0, 0, 255)));
}

#[test]
fn an_inline_image_reserves_only_its_own_cells() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}img{width:4ch;height:32px}</style><p><img alt=fallback> <a href=/image><img src=image.png alt=image></a> after</p>",
        24,
    );
    let image = page.painted.images.first().unwrap();
    assert_eq!(
        image.rect,
        textsurfer::layout::LayoutRect {
            col: 11,
            row: 0,
            width: 4,
            height: 2,
        }
    );
    assert_eq!(page.painted.text_lines(), ["", "[fallback]      after"]);
}

#[test]
fn resizing_recomputes_an_inline_images_atomic_box() {
    let mut load = load_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}img{width:50%;height:auto}</style><p><img src=image.png alt=image> after</p>",
        24,
    );
    let first = load.render_after_image().unwrap();
    assert_eq!(
        (
            first.painted.images[0].rect.width,
            first.painted.images[0].rect.height
        ),
        (12, 6)
    );
    let resized = load.resize(Size { cols: 12, rows: 24 }).unwrap();
    assert_eq!(
        (
            resized.painted.images[0].rect.width,
            resized.painted.images[0].rect.height
        ),
        (6, 3)
    );
}

#[test]
fn a_floated_image_reserves_every_cell_its_pixels_cover() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}figure{float:right;width:8ch}img{width:8ch;height:64px}</style><main><figure><a href=/image><img src=image.png alt=image></a><figcaption>caption</figcaption></figure><p>alpha beta gamma delta epsilon zeta eta theta iota kappa lambda</p></main>",
        24,
    );
    let image = page.painted.images.first().unwrap();
    assert_eq!((image.rect.width, image.rect.height), (8, 4));
    let lines = page.painted.text_lines();
    for (row, line) in lines
        .iter()
        .enumerate()
        .skip(image.rect.row)
        .take(image.rect.height)
    {
        assert!(
            line.chars()
                .skip(image.rect.col)
                .take(image.rect.width)
                .all(|character| character == ' '),
            "text occupies image cells on row {row} for {:?}: {:?}",
            image.rect,
            line
        );
    }
}

#[test]
fn a_floated_table_figure_wraps_its_caption_at_the_image_width() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}figure{display:table;float:right;border:1px solid}img{width:8ch;height:32px}figcaption{display:table-caption;caption-side:bottom}</style><main><figure><a href=/image><img src=image.png alt=image></a><figcaption>a long caption that wraps below</figcaption></figure><p>text beside the figure</p></main>",
        40,
    );
    let lines = page.painted.text_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.trim_end().ends_with("┌────────┐")),
        "caption max-content widened the floated figure beyond its eight-cell image: {lines:?}"
    );
}

#[test]
fn a_table_caption_min_content_can_widen_its_figure() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}figure{display:table;float:right;border:1px solid}img{width:8ch;height:32px}figcaption{display:table-caption;caption-side:bottom}</style><main><figure><img src=image.png alt=image><figcaption>unbreakable</figcaption></figure></main>",
        40,
    );
    let lines = page.painted.text_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.trim_end().ends_with("┌─────────┐")),
        "caption min-content did not widen the figure: {lines:?}"
    );
}

#[test]
fn authored_paragraph_lines_reclaim_the_measure_below_a_float() {
    let page = render_source(
        "<!doctype html><style>*{margin:0;padding:0}aside{float:right;width:6ch;height:32px}p{margin:0}</style><main><aside>FLOAT<br>FLOAT</aside><p>one two three four five six seven eight nine ten eleven twelve</p></main>",
        16,
    );
    let lines = page.painted.text_lines();
    assert!(lines.iter().take(2).all(|line| line.len() <= 16));
    assert!(
        lines
            .iter()
            .skip(2)
            .any(|line| line.trim_end().chars().count() > 10),
        "paragraph stayed constrained to the float-side slot: {lines:?}"
    );
}

#[test]
fn decoded_table_cell_images_keep_their_geometry_and_labels() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}table{border-spacing:0}td{padding:0}ul{list-style:none}li{display:inline}img{width:16px;height:16px}</style><table><tr><td><ul><li><a href=/list><img src=icon.png alt=icon>List</a></li><li><a href=/comparison><img src=icon.png alt=icon>Comparison</a></li><li><a href=/portal><img src=icon.png alt=icon>Portal</a></li><li><a href=/category><img src=icon.png alt=icon>Category</a></li></ul></td></tr></table>",
        40,
    );
    assert_eq!(page.painted.images.len(), 4);
    assert!(
        page.painted
            .images
            .iter()
            .all(|image| (image.rect.width, image.rect.height) == (2, 1))
    );
    let text = page.painted.text_lines().join("\n");
    assert!(!text.contains('\u{fffc}'));
    let labels = ["List", "Comparison", "Portal", "Category"];
    let positions = labels.map(|label| text.find(label).unwrap());
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    for href in ["/list", "/comparison", "/portal", "/category"] {
        let link = page
            .painted
            .links
            .iter()
            .find(|link| link.href == href)
            .unwrap();
        assert!(link.rects.iter().any(|rect| rect.width >= 2));
    }
}

#[test]
fn clipped_table_image_keeps_its_full_rect_and_separate_clip() {
    let page = render_source_with_image(
        "<!doctype html><style>*{margin:0;padding:0}table{border-spacing:0;table-layout:fixed;width:2ch}td{padding:0;overflow:hidden}img{width:32px;height:32px}</style><table><tr><td><a href=/image><img src=image.png alt=image></a></td></tr></table>",
        20,
    );
    let image = page.painted.images.first().unwrap();
    assert_eq!((image.rect.width, image.rect.height), (4, 2));
    assert_eq!((image.clip.width, image.clip.height), (2, 2));
    let link = page
        .painted
        .links
        .iter()
        .find(|link| link.href == "/image")
        .unwrap();
    assert!(link.rects.contains(&image.clip));
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
fn css_math_properties_golden() {
    insta::assert_snapshot!(golden("css_math.html"));
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
fn grid_layout_golden() {
    insta::assert_snapshot!(golden("grid.html"));
}

#[test]
fn floats_golden() {
    insta::assert_snapshot!(golden("floats.html"));
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
            render: RenderContext::terminal(Size { cols: 20, rows: 8 }),
            palette: palette(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
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
fn overlapping_grid_links_keep_dom_order_and_use_topmost_paint_order() {
    let page = render_source(
        "<style>body{margin:0}main{display:grid;grid-template-columns:6ch;width:6ch}a{grid-area:1 / 1}</style><main><a href=first>FIRST</a><a href=second>SECOND</a></main>",
        20,
    );
    assert_eq!(page.painted.links[0].href, "first");
    assert_eq!(page.painted.links[1].href, "second");
    assert_eq!(page.painted.links[0].rects[0].col, 0);
    assert_eq!(page.painted.links[1].rects[0].col, 0);
    assert_eq!(page.painted.links[0].rects[0].row, 0);
    assert_eq!(page.painted.links[1].rects[0].row, 0);
    assert_eq!(
        page.painted.link_at(1, 0).map(|link| link.href.as_str()),
        Some("second")
    );
}

#[test]
fn presentational_html_golden() {
    insta::assert_snapshot!(golden("presentational.html"));
}

#[test]
fn form_controls_golden() {
    insta::assert_snapshot!(golden("forms.html"));
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
fn collapsed_table_borders_use_the_table_background() {
    let page = render_source(
        "<style>body{margin:0} table{border-collapse:collapse;background:#400000;border:1px solid #000;margin:0}
         td{background:#004000;border-top:1px solid #000;border-bottom:1px solid #000;padding:0 1ch;font-weight:bold}</style>
         <table><tr><td>cell</td></tr></table>",
        20,
    );
    let cell_background = Rgb::new(0, 64, 0);
    let border_background = Rgb::new(64, 0, 0);
    let foreground = Rgba::opaque(legible_foreground(Rgb::BLACK, border_background, palette()));
    let mut border_cells = 0;
    let mut saw_text = false;
    for span in page.painted.rows.iter().flat_map(|row| &row.spans) {
        let borders = span
            .text
            .chars()
            .filter(|character| "┌┐└┘─│├┤┬┴┼".contains(*character))
            .count();
        if borders > 0 {
            border_cells += borders;
            assert_eq!(span.style.bg, Some(border_background));
            assert_eq!(span.style.fg, Some(foreground));
            assert!(!span.style.bold);
            assert!(!span.style.underline);
            assert!(!span.style.strike);
            assert!(!span.style.reverse);
            assert!(!span.style.dim);
            assert_eq!(span.style.scale, 1);
        }
        if span
            .text
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        {
            saw_text = true;
            assert_eq!(span.style.bg, Some(cell_background));
            assert!(span.style.bold);
        }
    }
    assert!(border_cells >= 10);
    assert!(saw_text);
}

#[test]
fn collapsed_borders_of_a_transparent_table_expose_the_ancestor_background() {
    let page = render_source(
        "<style>body{margin:0;background:#202020}table{border-collapse:collapse}td{background:#004000;border:1px solid #000;padding:0 1ch}</style><table><tr><td>A</td></tr></table>",
        20,
    );
    for span in page.painted.rows.iter().flat_map(|row| &row.spans) {
        if span
            .text
            .chars()
            .any(|character| "┌┐└┘─│├┤┬┴┼".contains(character))
        {
            assert_eq!(span.style.bg, Some(Rgb::new(32, 32, 32)));
        }
    }
}

#[test]
fn linux_layers_table_uses_originating_row_and_cell_backgrounds() {
    let page = render_source(
        "<style>body{margin:0}table{border-collapse:collapse;border:1px solid #000;background:#202020}td{border:1px solid #000;padding:0 1ch}.user{background:#004000}.kernel{background:#400000}.hardware{background:#804000}.system{background:#000040}.other{background:#404000}</style>
         <table><tr class=user><td rowspan=2>Mode</td><td class=system>System</td><td>Shell</td></tr><tr class=user><td>Library</td><td class=other>Other</td></tr><tr class=kernel><td colspan=3>Kernel</td></tr><tr class=hardware><td colspan=3>Hardware</td></tr></table>",
        40,
    );
    let background = |text: &str| {
        page.painted
            .rows
            .iter()
            .flat_map(|row| &row.spans)
            .find(|span| span.text.contains(text))
            .and_then(|span| span.style.bg)
    };
    assert_eq!(background("Mode"), Some(Rgb::new(0, 64, 0)));
    assert_eq!(background("System"), Some(Rgb::new(0, 0, 64)));
    assert_eq!(background("Shell"), Some(Rgb::new(0, 64, 0)));
    assert_eq!(background("Library"), Some(Rgb::new(0, 64, 0)));
    assert_eq!(background("Other"), Some(Rgb::new(64, 64, 0)));
    assert_eq!(background("Kernel"), Some(Rgb::new(64, 0, 0)));
    assert_eq!(background("Hardware"), Some(Rgb::new(128, 64, 0)));
    for span in page.painted.rows.iter().flat_map(|row| &row.spans) {
        if span
            .text
            .chars()
            .any(|character| "┌┐└┘─│├┤┬┴┼".contains(character))
        {
            assert_eq!(span.style.bg, Some(Rgb::new(32, 32, 32)));
        }
    }
}

#[test]
fn collapsed_shared_border_backdrops_survive_source_capture() {
    let page = render_source(
        "<style>body{margin:0}table{border-collapse:collapse;background:#202020}td{border:1px solid #000;padding:0}.left{background:#400000}.right{background:#004000}</style><table><tr><td class=left>A</td><td class=right>B</td></tr></table>",
        20,
    );
    let row = page
        .painted
        .rows
        .iter()
        .find(|row| {
            let text = row
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>();
            text.contains('A') && text.contains('B')
        })
        .unwrap();
    let shared = row
        .spans
        .iter()
        .find(|span| span.text.contains('│') && span.col > 0)
        .unwrap();
    assert_eq!(shared.style.bg, Some(Rgb::new(32, 32, 32)));
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
