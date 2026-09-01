use super::*;
use crate::core::geom::Size;
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};
use crate::html::{Html5everParser, HtmlParser};
use crate::layout::LayoutRect;
use proptest::prelude::*;
use unicode_width::UnicodeWidthStr;

/// Layout inputs for a page nobody has typed into: the authored state.
fn test_input<'a>(document: &'a Document, styles: &'a StyleTree) -> LayoutInput<'a> {
    LayoutInput {
        document,
        styles,
        forms: FormState::empty(),
        images: None,
        cell_metric: crate::core::style::CellMetric::DEFAULT,
    }
}

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
    let styles = StyloCascade.apply(
        &sheets,
        &document,
        MediaContext::screen().with_viewport(Size {
            cols: width as u16,
            rows: 24,
        }),
    );
    let table = document.element_by_id("table").unwrap();
    TableFormatter::new(test_input(&document, &styles)).format(
        table,
        width,
        TableLimits::default(),
        0,
    )
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
fn a_row_background_is_contributed_only_through_real_cells() {
    let output = formatted(
        "<style id=css>#table{border-spacing:0;background:#000040} tr:first-child{background:#400000} td{padding:0}</style>
         <table id=table><tr><td>A</td></tr><tr><td>B</td><td>wide</td></tr></table>",
        30,
    );
    let cells: Vec<_> = output.boxes.iter().filter(|box_| box_.depth == 5).collect();
    assert_eq!(cells.len(), 3);
    let first = cells[0].border_rect;
    let missing = LayoutRect {
        col: cells[2].border_rect.col,
        row: first.row,
        width: 1,
        height: 1,
    };
    let row_color = crate::core::style::Rgb::new(64, 0, 0);
    assert!(output.fills.iter().any(|fill| {
        fill.color == Some(row_color)
            && fill.rect.col <= first.col
            && fill.rect.col.saturating_add(fill.rect.width) > first.col
            && fill.rect.row <= first.row
            && fill.rect.row.saturating_add(fill.rect.height) > first.row
    }));
    assert!(!output.fills.iter().any(|fill| {
        fill.color == Some(row_color)
            && fill.rect.col <= missing.col
            && fill.rect.col.saturating_add(fill.rect.width) > missing.col
            && fill.rect.row <= missing.row
            && fill.rect.row.saturating_add(fill.rect.height) > missing.row
    }));
}

#[test]
fn anonymous_row_groups_keep_their_source_position() {
    let output = formatted(
        "<style id=css>#table{display:table}.row{display:table-row}.group{display:table-row-group}.cell{display:table-cell}</style>
         <div id=table><div class=row><span class=cell>A</span></div><div class=group><div class=row><span class=cell>B</span></div></div></div>",
        30,
    );
    assert_eq!(output.plain_text(), "A B");
}

#[test]
fn positive_rowspans_stop_at_their_row_group_boundary() {
    let output = formatted(
        "<table id=table><tbody><tr><td rowspan=9>A</td></tr></tbody><tbody><tr><td>B</td></tr></tbody></table>",
        30,
    );
    assert_eq!(output.model.cells[0].row_span, 1);
    assert_eq!(output.model.cells[1].col, 0);
}

#[test]
fn an_empty_colgroup_contributes_one_group_layer_not_a_column_layer() {
    let output = formatted(
        "<style id=css>#table{border-spacing:0} colgroup{background:#400000} td{padding:0}</style>
         <table id=table><colgroup span=2><tr><td>A</td><td>B</td></tr></table>",
        30,
    );
    let color = crate::core::style::Rgb::new(64, 0, 0);
    assert_eq!(
        output
            .fills
            .iter()
            .filter(|fill| fill.color == Some(color))
            .count(),
        2
    );
}

#[test]
fn rtl_tables_place_the_first_logical_cell_at_inline_start() {
    let output = formatted(
        "<style id=css>#table{direction:rtl;border-spacing:0} td{padding:0}</style><table id=table><tr><td>A</td><td>B</td></tr></table>",
        30,
    );
    assert_eq!(output.plain_text(), "B A");
}

#[test]
fn empty_cells_hide_suppresses_only_empty_separated_cells() {
    let output = formatted(
        "<style id=css>#table{border-spacing:0} td{empty-cells:hide;background:#400000;border:solid;padding:0}</style>
         <table id=table><tr><td></td><td>B</td></tr></table>",
        30,
    );
    assert_eq!(
        output.boxes.iter().filter(|box_| box_.depth == 5).count(),
        1
    );
}

#[test]
fn collapsed_rows_remove_their_track_space_after_sizing() {
    let collapsed = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0}.gone{visibility:collapse}</style><table id=table><tr><td>A</td></tr><tr class=gone><td>hidden</td></tr><tr><td>C</td></tr></table>",
        30,
    );
    let reference = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0}</style><table id=table><tr><td>A</td></tr><tr><td>C</td></tr></table>",
        30,
    );
    assert_eq!(collapsed.height, reference.height);
}

#[test]
fn collapsed_columns_remove_their_track_space_after_sizing() {
    let collapsed = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0}.gone{visibility:collapse}</style><table id=table><col class=gone><col><tr><td>very-wide-hidden</td><td>B</td></tr></table>",
        40,
    );
    let reference = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0}</style><table id=table><tr><td>B</td></tr></table>",
        40,
    );
    assert_eq!(collapsed.width, reference.width);
}

#[test]
fn definite_cell_and_row_heights_are_track_minima() {
    let cell = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0;height:48px}</style><table id=table><tr><td>A</td></tr></table>",
        30,
    );
    let row = formatted(
        "<style id=css>#table{border-spacing:0}td{padding:0}tr{height:64px}</style><table id=table><tr><td>A</td></tr></table>",
        30,
    );
    assert_eq!(cell.height, 3);
    assert_eq!(row.height, 4);
}

#[test]
fn a_definite_table_height_distributes_only_required_extra_space() {
    let output = formatted(
        "<style id=css>#table{border-spacing:0;height:64px}td{padding:0}</style><table id=table><tr><td>A</td></tr><tr><td>B</td></tr></table>",
        30,
    );
    assert_eq!(output.height, 4);
    assert_eq!(output.model.rows.len(), 2);
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
fn repeated_inline_table_formatting_reuses_one_cached_output() {
    let outcome = Html5everParser::new(false).parse_document(
        "<table id=table><tr><td><table id=inner><tr><td>nested</td></tr></table></td></tr></table>",
    );
    let document = outcome.document.borrow();
    let styles = StyloCascade.apply(&[], &document, MediaContext::screen());
    let inner = document.element_by_id("inner").unwrap();
    let formatter = TableFormatter::new(test_input(&document, &styles));
    let first = formatter.format_inline_atom(inner, 40, TableLimits::default(), 1);
    assert_eq!(formatter.cached_outputs(), 1);
    let second = formatter.format_inline_atom(inner, 40, TableLimits::default(), 1);
    assert_eq!(formatter.cached_outputs(), 1);
    assert_eq!((first.width, first.height), (second.width, second.height));
    assert_eq!(first.plain_text(), second.plain_text());
}

#[test]
fn constrained_auto_tables_shrink_min_content_columns_inside_their_target_width() {
    let output = formatted(
        "<style id=css>#table { width:100%;border:solid;padding:1ch } td { white-space:nowrap }</style>
         <table id=table><tr><td>abcdefghijklmno</td><td>pqrstuvwxyz</td></tr></table>",
        20,
    );
    assert!(output.width <= 20);
    assert!(output.fragments.iter().all(|fragment| {
        fragment
            .col
            .saturating_add(UnicodeWidthStr::width(fragment.text.as_str()))
            <= output.width
    }));
    assert!(
        output
            .strokes
            .iter()
            .all(|stroke| stroke.rect.col.saturating_add(stroke.rect.width) <= output.width)
    );
}

#[test]
fn nested_tables_use_their_minimum_width_without_losing_spanning_content() {
    for (inner_style, caption) in [
        ("", ""),
        ("style='width:100%'", ""),
        (
            "",
            "<caption>alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu</caption>",
        ),
    ] {
        let source = format!(
            "<div><table id=table><tr><td><table {inner_style}>{caption}
             <tr><th rowspan=3>User mode</th><td>alpha one</td><td>beta two</td><td>gamma three</td></tr>
             <tr><td colspan=3>MARKER-A: components ALSA, DRI, evdev, klibc, LVM</td></tr>
             </table></td></tr></table></div>"
        );
        for width in [40, 60] {
            let output = formatted(&source, width);
            assert!(
                output.width <= width,
                "{inner_style:?} at {width} columns produced width {}",
                output.width
            );
            let text = output.plain_text();
            for token in [
                "gamma",
                "three",
                "MARKER-A:",
                "ALSA,",
                "DRI,",
                "evdev,",
                "klibc,",
                "LVM",
            ] {
                assert!(
                    text.split_whitespace().any(|word| word == token),
                    "missing {token:?} from {inner_style:?} at {width} columns: {text:?}"
                );
            }
            if width == 40 {
                let marker_row = output
                    .fragments
                    .iter()
                    .find(|fragment| fragment.text.contains("MARKER-A"))
                    .map(|fragment| fragment.row)
                    .unwrap();
                let end_row = output
                    .fragments
                    .iter()
                    .find(|fragment| fragment.text.contains("LVM"))
                    .map(|fragment| fragment.row)
                    .unwrap();
                assert!(end_row > marker_row);
            }
        }
    }
}

#[test]
fn a_definite_nested_table_width_remains_non_compressible() {
    let output = formatted(
        "<table id=table><tr><td><table style='width:80ch'><tr><td>wide</td></tr></table></td></tr></table>",
        40,
    );
    assert!(output.width >= 80);
}

#[test]
fn percentage_columns_are_clamped_before_final_width_distribution() {
    let output = formatted(
        "<style id=css>#table { width:100ch; border-collapse:collapse }</style>
         <table id=table>
         <tr><td style='width:10%'>top-a</td><td style='width:10%'>top-b</td><td colspan=5 style='width:80%'>span</td></tr>
         <tr><td style='width:10%'>A</td><td style='width:10%'>B</td><td style='width:15%'>C</td><td style='width:15%'>D</td><td style='width:15%'>E</td><td style='width:15%'>F</td><td style='width:30%'>G</td></tr>
         </table>",
        100,
    );
    let width = |marker: &str| {
        let fragment = output
            .fragments
            .iter()
            .find(|fragment| fragment.text == marker)
            .unwrap();
        fragment.clip_right - fragment.col
    };
    assert!(width("G") <= width("A") * 2 + 1);
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
    let styles = StyloCascade.apply(&[], &document, MediaContext::screen());
    let table = document.element_by_id("table").unwrap();
    let limited = TableFormatter::new(test_input(&document, &styles)).format(
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
        "<div id=parent><span id=a style='display:table-cell'>A</span><span id=b style='display:table-cell'>B</span></div>",
    );
    let document = outcome.document.borrow();
    let styles = StyloCascade.apply(&[], &document, MediaContext::screen());
    let parent = document.element_by_id("parent").unwrap();
    let anonymous = TableFormatter::new(test_input(&document, &styles)).format_anonymous(
        vec![
            document.element_by_id("a").unwrap(),
            document.element_by_id("b").unwrap(),
        ],
        styles.get(parent),
        false,
        30,
        TableLimits {
            max_columns: 1,
            ..Default::default()
        },
        0,
    );
    assert!(anonymous.degraded);
    assert_eq!(anonymous.plain_text(), "A B");

    let outcome = Html5everParser::new(false).parse_document(
        "<table id=table><tr><td>before<table><tr><td>middle<table><tr><td>deep</td></tr></table>after-middle</td></tr></table>after</td></tr></table>",
    );
    let document = outcome.document.borrow();
    let styles = StyloCascade.apply(&[], &document, MediaContext::screen());
    let table = document.element_by_id("table").unwrap();
    let nested = TableFormatter::new(test_input(&document, &styles)).format(
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
fn the_widest_collapsed_border_wins_before_line_style() {
    let output = formatted(
        "<style id=css>#table{border-collapse:collapse}.left{border-right:1px solid #400000}.right{border-left:5px dotted #000040}</style>
         <table id=table><tr><td class=left>A</td><td class=right>B</td></tr></table>",
        30,
    );
    let cells: Vec<_> = output.boxes.iter().filter(|box_| box_.depth == 5).collect();
    let shared_col = cells[0]
        .border_rect
        .col
        .saturating_add(cells[0].border_rect.width)
        .saturating_sub(1);
    let winner = output
        .strokes
        .iter()
        .find(|stroke| stroke.edges.left.layout_width() > 0)
        .unwrap_or_else(|| panic!("shared column {shared_col}, strokes: {:?}", output.strokes));
    assert_eq!(
        winner.edges.left.color,
        crate::core::style::BorderColor::Rgb(crate::core::style::Rgb::new(0, 0, 64))
    );
    assert_eq!(
        winner.edges.left.style,
        crate::core::style::BorderLineStyle::Dotted
    );
}

#[test]
fn collapsed_shared_borders_expose_only_the_table_background() {
    let output = formatted(
        "<style id=css>#table{border-collapse:collapse;background:#202020}td{border:solid;padding:0}.left{background:#400000}.right{background:#004000}</style>
         <table id=table><tr><td class=left>A</td><td class=right>B</td></tr></table>",
        30,
    );
    let cells: Vec<_> = output.boxes.iter().filter(|box_| box_.depth == 5).collect();
    let shared_col = cells[1].border_rect.col;
    let row = cells[1].border_rect.row.saturating_add(1);
    let covers = |color, fill: &&crate::layout::BackgroundFill| {
        fill.color == Some(color)
            && fill.rect.col <= shared_col
            && fill.rect.col.saturating_add(fill.rect.width) > shared_col
            && fill.rect.row <= row
            && fill.rect.row.saturating_add(fill.rect.height) > row
    };
    assert!(
        output
            .fills
            .iter()
            .any(|fill| covers(crate::core::style::Rgb::new(32, 32, 32), &fill))
    );
    assert!(
        !output
            .fills
            .iter()
            .any(|fill| covers(crate::core::style::Rgb::new(64, 0, 0), &fill))
    );
    assert!(
        !output
            .fills
            .iter()
            .any(|fill| covers(crate::core::style::Rgb::new(0, 64, 0), &fill))
    );
}

#[test]
fn collapsed_transparent_shared_borders_keep_normal_background_ownership() {
    let output = formatted(
        "<style id=css>#table{border-collapse:collapse;background:#202020}td{padding:0;border:solid}.left{background:#400000;border-right-color:transparent}.right{background:#004000;border-left-color:transparent}</style>
         <table id=table><tr><td class=left>A</td><td class=right>B</td></tr></table>",
        30,
    );
    let cells: Vec<_> = output.boxes.iter().filter(|box_| box_.depth == 5).collect();
    let shared_col = cells[1].border_rect.col;
    let row = cells[1].border_rect.row.saturating_add(1);
    let covers = |color, fill: &&crate::layout::BackgroundFill| {
        fill.color == Some(color)
            && fill.rect.col <= shared_col
            && fill.rect.col.saturating_add(fill.rect.width) > shared_col
            && fill.rect.row <= row
            && fill.rect.row.saturating_add(fill.rect.height) > row
    };
    assert!(
        output
            .fills
            .iter()
            .any(|fill| covers(crate::core::style::Rgb::new(64, 0, 0), &fill))
    );
}

#[test]
fn collapsed_empty_cells_do_not_recreate_fully_masked_backgrounds() {
    let output = formatted(
        "<style id=css>#table{border-collapse:collapse;background:#202020}td{padding:0;border:solid;background:#400000}</style>
         <table id=table><tr><td></td></tr></table>",
        30,
    );
    let cell = output.boxes.iter().find(|box_| box_.depth == 5).unwrap();
    assert!(cell.background_handled);
    assert!(
        output
            .fills
            .iter()
            .filter(|fill| fill.color == Some(crate::core::style::Rgb::new(64, 0, 0)))
            .all(|fill| {
                fill.rect.col > cell.border_rect.col
                    && fill.rect.row > cell.border_rect.row
                    && fill.rect.col.saturating_add(fill.rect.width)
                        < cell.border_rect.col.saturating_add(cell.border_rect.width)
                    && fill.rect.row.saturating_add(fill.rect.height)
                        < cell.border_rect.row.saturating_add(cell.border_rect.height)
            })
    );
}

#[test]
fn collapsed_transparent_current_color_does_not_mask_cell_backgrounds() {
    let output = formatted(
        "<style id=css>#table{border-collapse:collapse}td{padding:0;border:solid currentColor;color:transparent;background:#400000}</style>
         <table id=table><tr><td>A</td></tr></table>",
        30,
    );
    let cell = output.boxes.iter().find(|box_| box_.depth == 5).unwrap();
    assert!(output.fills.iter().any(|fill| {
        fill.color == Some(crate::core::style::Rgb::new(64, 0, 0)) && fill.rect == cell.border_rect
    }));
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

#[test]
fn disjoint_table_rectangles_return_no_intersection_without_subtracting() {
    let left = LayoutRect {
        col: 20,
        row: 10,
        width: 4,
        height: 2,
    };
    let right = LayoutRect {
        col: 0,
        row: 0,
        width: 5,
        height: 3,
    };
    assert_eq!(super::geometry::intersect_rect(left, right), None);
    let above = LayoutRect {
        col: 20,
        row: 0,
        width: 4,
        height: 3,
    };
    assert_eq!(super::geometry::intersect_rect(left, above), None);
}
