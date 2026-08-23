use super::*;
use crate::core::geom::Size;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::html::{Html5everParser, HtmlParser};
use crate::layout::LayoutRect;
use proptest::prelude::*;
use unicode_width::UnicodeWidthStr;

fn formatted(source: &str, width: usize) -> TableOutput {
    let outcome = Html5everParser::new(false).parse_document(source);
    let document = outcome.document.borrow();
    let sheets = vec![
        CssparserParser.parse(
            document
                .element_by_id("css")
                .and_then(|id| document.first_child(id))
                .and_then(|id| match document.node(id) {
                    Some(crate::core::dom::Node::Text { data }) => Some(data.as_str()),
                    _ => None,
                })
                .unwrap_or_default(),
        ),
    ];
    let styles = BasicCascade.apply(
        &sheets,
        &document,
        MediaContext::screen().with_viewport(Size {
            cols: width as u16,
            rows: 24,
        }),
    );
    let table = document.element_by_id("table").unwrap();
    TableFormatter::new(&document, &styles).format(table, width, TableLimits::default(), 0)
}

#[test]
fn spans_form_a_sparse_non_overlapping_grid() {
    let output = formatted(
        "<table id=table><tbody><tr><td rowspan=2>A</td><td>B</td></tr>
         <tr><td>C</td></tr><tr><td colspan=2>D</td></tr></tbody></table>",
        30,
    );
    assert_eq!(output.model.rows.len(), 3);
    assert_eq!(output.model.columns, 2);
    assert_eq!(output.model.cells[0].row_span, 2);
    assert_eq!(output.model.cells[3].col_span, 2);
    for (index, left) in output.model.cells.iter().enumerate() {
        for right in output.model.cells.iter().skip(index + 1) {
            assert!(
                left.row + left.row_span <= right.row
                    || right.row + right.row_span <= left.row
                    || left.col + left.col_span <= right.col
                    || right.col + right.col_span <= left.col
            );
        }
    }
}

#[test]
fn image_alt_fallbacks_match_inside_table_cells() {
    let output = formatted(
        "<table id=table><tr><td>A<img alt=''>B<img alt='   '>C<img alt='cat'><img></td></tr></table>",
        40,
    );
    let mut fragments: Vec<_> = output.fragments.iter().collect();
    fragments.sort_by_key(|fragment| (fragment.row, fragment.col));
    let text = fragments
        .into_iter()
        .map(|fragment| fragment.text.as_str())
        .collect::<String>();
    assert_eq!(text, "ABC[cat][img]");
}

#[test]
fn fixed_layout_clips_wide_graphemes_at_the_cell_inner_edge() {
    let output = formatted(
        "<style id=css>#table { table-layout: fixed; width: 5ch } td { border: solid }</style>
         <table id=table><tr><td>ab界z</td><td>x</td></tr></table>",
        30,
    );
    assert!(output.width >= 5);
    assert!(output.fragments.iter().all(|fragment| {
        fragment.col + unicode_width::UnicodeWidthStr::width(fragment.text.as_str())
            <= fragment.clip_right
    }));
    assert!(
        output
            .fragments
            .iter()
            .all(|fragment| !fragment.text.contains('…'))
    );
}

#[test]
fn table_min_content_width_is_the_widest_unbreakable_word() {
    let output = formatted(
        "<style id=css>#table { border-spacing: 0 } td { padding: 0 }</style>
         <table id=table><tr><td>small elephant ox</td></tr></table>",
        1,
    );
    assert_eq!(output.width, "elephant".len());
}

#[test]
fn collapsed_spans_share_one_connected_border_group() {
    let output = formatted(
        "<style id=css>#table { border-collapse: collapse } td { border: solid red }</style>
         <table id=table><tr><td colspan=2>A</td></tr><tr><td>B</td><td>C</td></tr></table>",
        30,
    );
    assert!(!output.strokes.is_empty());
    assert!(
        output
            .strokes
            .iter()
            .all(|stroke| stroke.merge_group == output.strokes[0].merge_group)
    );
}

#[test]
fn resource_limit_falls_back_without_losing_text() {
    let output = formatted("<table id=table><tr><td>A</td><td>B</td></tr></table>", 30);
    let outcome = Html5everParser::new(false)
        .parse_document("<table id=table><tr><td>A</td><td>B</td></tr></table>");
    let document = outcome.document.borrow();
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let table = document.element_by_id("table").unwrap();
    let limited = TableFormatter::new(&document, &styles).format(
        table,
        30,
        TableLimits {
            max_columns: 1,
            ..Default::default()
        },
        0,
    );
    assert!(!output.fragments.is_empty());
    assert!(limited.degraded);
    assert_eq!(limited.plain_text(), "A B");

    let outcome = Html5everParser::new(false).parse_document(
        "<table id=table><tr><td>before<table><tr><td>middle<table><tr><td>deep</td></tr></table>after-middle</td></tr></table>after</td></tr></table>",
    );
    let document = outcome.document.borrow();
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let table = document.element_by_id("table").unwrap();
    let nested = TableFormatter::new(&document, &styles).format(
        table,
        30,
        TableLimits {
            max_nesting: 1,
            ..Default::default()
        },
        0,
    );
    assert_eq!(nested.plain_text(), "before middle deep after-middle after");
}

#[test]
fn authored_table_cells_receive_anonymous_row_fixup() {
    let output = formatted(
        "<style id=css>#table { display: table } .cell { display: table-cell }</style>
         <div id=table><span class=cell>A</span><span class=cell>B</span></div>",
        30,
    );
    assert_eq!(output.model.rows.len(), 1);
    assert_eq!(output.model.columns, 2);
    assert_eq!(output.plain_text(), "A B");
}

#[test]
fn zero_rowspan_stops_at_the_explicit_group_boundary() {
    let output = formatted(
        "<table id=table><tbody><tr><td rowspan=0>A</td><td>B</td></tr><tr><td>C</td></tr></tbody>
         <tbody><tr><td>D</td><td>E</td></tr></tbody></table>",
        30,
    );
    assert_eq!(output.model.cells[0].row_span, 2);
    assert_eq!(output.model.rows.len(), 3);
}

#[test]
fn hidden_collapsed_edge_suppresses_the_competing_visible_edge() {
    let output = formatted(
        "<style id=css>#table { border-collapse: collapse }
         td { border: solid } .left { border-right-style: hidden }</style>
         <table id=table><tr><td class=left>A</td><td>B</td></tr></table>",
        30,
    );
    let left_node = output.model.cells[0].owner.unwrap();
    let left_box = output
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == left_node)
        .unwrap();
    let boundary = left_box.border_rect.col + left_box.border_rect.width - 1;
    assert!(
        !output
            .strokes
            .iter()
            .any(|stroke| stroke.rect.col == boundary && stroke.rect.width == 1)
    );
}

#[test]
fn table_hit_boxes_are_laminar_and_text_cells_are_disjoint() {
    let output = formatted(
        "<style id=css>#table { border-collapse: collapse } td { border: solid }</style>
         <table id=table><tr><td rowspan=2>A</td><td>B</td></tr><tr><td>C</td></tr></table>",
        30,
    );
    for (index, left) in output.boxes.iter().enumerate() {
        for right in output.boxes.iter().skip(index + 1) {
            if rects_intersect(left.border_rect, right.border_rect) {
                assert!(
                    rect_contains(left.border_rect, right.border_rect)
                        || rect_contains(right.border_rect, left.border_rect)
                );
            }
        }
    }
    let mut occupied = std::collections::HashSet::new();
    for fragment in &output.fragments {
        for col in fragment.col..fragment.col + UnicodeWidthStr::width(fragment.text.as_str()) {
            assert!(occupied.insert((fragment.row, col)));
        }
    }
}

proptest! {
    #[test]
    fn generated_spans_never_overlap(
        spans in prop::collection::vec(1usize..=4, 1..24),
        width in 8usize..80,
    ) {
        let cells = spans
            .iter()
            .enumerate()
            .map(|(index, span)| format!("<td colspan={span}>{index}</td>"))
            .collect::<String>();
        let source = format!("<table id=table><tr>{cells}</tr></table>");
        let output = formatted(&source, width);
        for (index, left) in output.model.cells.iter().enumerate() {
            for right in output.model.cells.iter().skip(index + 1) {
                prop_assert!(
                    left.row + left.row_span <= right.row
                        || right.row + right.row_span <= left.row
                        || left.col + left.col_span <= right.col
                        || right.col + right.col_span <= left.col
                );
            }
        }
    }

    #[test]
    fn wider_table_viewports_do_not_increase_height(
        narrow in 8usize..40,
        extra in 0usize..40,
    ) {
        let source = "<table id=table><tr><td>alpha beta gamma delta</td><td>one two three four</td></tr></table>";
        let narrow_output = formatted(source, narrow);
        let wide_output = formatted(source, narrow + extra);
        prop_assert!(wide_output.height <= narrow_output.height);
    }
}

fn rects_intersect(left: LayoutRect, right: LayoutRect) -> bool {
    left.col < right.col + right.width
        && right.col < left.col + left.width
        && left.row < right.row + right.height
        && right.row < left.row + left.height
}

fn rect_contains(outer: LayoutRect, inner: LayoutRect) -> bool {
    outer.col <= inner.col
        && outer.row <= inner.row
        && outer.col + outer.width >= inner.col + inner.width
        && outer.row + outer.height >= inner.row + inner.height
}
