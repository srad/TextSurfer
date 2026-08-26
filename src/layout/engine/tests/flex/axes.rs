use super::{box_for, flex_fixture};
use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::layout::LayoutRect;
use taffy::style::{
    AlignContentKeyword as TaffyContentKeyword, AlignItemsKeyword as TaffyItemKeyword,
    AlignmentSafety as TaffySafety,
};

fn disjoint(a: LayoutRect, b: LayoutRect) -> bool {
    a.col + a.width <= b.col
        || b.col + b.width <= a.col
        || a.row + a.height <= b.row
        || b.row + b.height <= a.row
}

#[test]
fn flex_wrap_and_gap_create_stable_rows() {
    let fixture = flex_fixture(
        "flex-wrap:wrap;column-gap:1ch;row-gap:16px;width:5ch",
        "width:2ch;flex:none",
        3,
        Size { cols: 20, rows: 8 },
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[0]).border_rect.col, 0);
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.col, 3);
    assert_eq!(box_for(&fixture.tree, fixture.items[2]).border_rect.row, 2);
}

#[test]
fn every_flex_direction_places_fixed_items_on_the_expected_main_axis() {
    for (direction, expected) in [
        ("row", [(0, 0), (2, 0)]),
        ("row-reverse", [(8, 0), (6, 0)]),
        ("column", [(0, 0), (0, 1)]),
        ("column-reverse", [(0, 3), (0, 2)]),
    ] {
        let fixture = flex_fixture(
            &format!("width:10ch;height:64px;flex-direction:{direction}"),
            "width:2ch;height:16px;flex:none",
            2,
            Size { cols: 20, rows: 8 },
        );
        let actual: Vec<_> = fixture
            .items
            .iter()
            .map(|&node| {
                let rect = box_for(&fixture.tree, node).border_rect;
                (rect.col, rect.row)
            })
            .collect();
        assert_eq!(actual, expected, "{direction}");
    }
}

#[test]
fn every_direction_and_wrap_combination_preserves_axes_and_reversals() {
    for direction in ["row", "row-reverse", "column", "column-reverse"] {
        for wrap in ["nowrap", "wrap", "wrap-reverse"] {
            let is_column = direction.starts_with("column");
            let is_reverse = direction.ends_with("reverse");
            let is_wrapped = wrap != "nowrap";
            let main_size = if is_wrapped { 3 } else { 5 };
            let container = if is_column {
                format!(
                    "width:10ch;height:{}px;flex-direction:{direction};flex-wrap:{wrap};row-gap:16px;column-gap:2ch;align-content:flex-start;align-items:flex-start",
                    main_size * 16
                )
            } else {
                format!(
                    "width:{main_size}ch;height:80px;flex-direction:{direction};flex-wrap:{wrap};row-gap:16px;column-gap:1ch;align-content:flex-start;align-items:flex-start"
                )
            };
            let fixture = flex_fixture(
                &container,
                "width:1ch;height:16px;flex:none",
                3,
                Size { cols: 20, rows: 8 },
            );
            let rects: Vec<_> = fixture
                .items
                .iter()
                .map(|&node| box_for(&fixture.tree, node).border_rect)
                .collect();
            assert!(disjoint(rects[0], rects[1]), "{direction} {wrap}");
            assert!(disjoint(rects[0], rects[2]), "{direction} {wrap}");
            assert!(disjoint(rects[1], rects[2]), "{direction} {wrap}");
            let main = |rect: LayoutRect| if is_column { rect.row } else { rect.col };
            let cross = |rect: LayoutRect| if is_column { rect.col } else { rect.row };
            assert_eq!(
                main(rects[0]) > main(rects[1]),
                is_reverse,
                "{direction} {wrap}"
            );
            assert_eq!(cross(rects[0]), cross(rects[1]), "{direction} {wrap}");
            if is_wrapped {
                assert_eq!(main(rects[0]), main(rects[2]), "{direction} {wrap}");
                assert_eq!(
                    cross(rects[2]) < cross(rects[0]),
                    wrap == "wrap-reverse",
                    "{direction} {wrap}"
                );
            } else {
                assert_eq!(cross(rects[0]), cross(rects[2]), "{direction} {wrap}");
            }
        }
    }
}

#[test]
fn justify_content_distributes_free_space_for_every_distribution_family() {
    for (value, expected) in [
        ("normal", [0, 2]),
        ("stretch", [0, 2]),
        ("start", [0, 2]),
        ("flex-start", [0, 2]),
        ("left", [0, 2]),
        ("center", [4, 6]),
        ("safe center", [4, 6]),
        ("unsafe center", [4, 6]),
        ("end", [8, 10]),
        ("flex-end", [8, 10]),
        ("right", [8, 10]),
        ("space-between", [0, 10]),
        ("space-around", [2, 8]),
        ("space-evenly", [3, 7]),
    ] {
        let fixture = flex_fixture(
            &format!("width:12ch;justify-content:{value}"),
            "width:2ch;height:16px;flex:none",
            2,
            Size { cols: 20, rows: 5 },
        );
        let actual: Vec<_> = fixture
            .items
            .iter()
            .map(|&node| box_for(&fixture.tree, node).border_rect.col)
            .collect();
        assert_eq!(actual, expected, "{value}");
    }
}

#[test]
fn cross_axis_alignment_covers_item_alignment_families() {
    for (value, expected_row) in [
        ("start", 0),
        ("flex-start", 0),
        ("self-start", 0),
        ("center", 2),
        ("safe center", 2),
        ("unsafe center", 2),
        ("end", 3),
        ("flex-end", 3),
        ("self-end", 3),
    ] {
        let fixture = flex_fixture(
            &format!("height:64px;align-items:{value}"),
            "width:2ch;height:16px;flex:none",
            1,
            Size { cols: 20, rows: 8 },
        );
        assert_eq!(
            box_for(&fixture.tree, fixture.items[0]).border_rect.row,
            expected_row,
            "{value}"
        );
    }
    for value in ["stretch", "normal"] {
        let fixture = flex_fixture(
            &format!("height:64px;align-items:{value}"),
            "width:2ch;flex:none",
            1,
            Size { cols: 20, rows: 8 },
        );
        assert_eq!(
            box_for(&fixture.tree, fixture.items[0]).border_rect.height,
            4,
            "{value}"
        );
    }
    let overridden = flex_fixture(
        "height:64px;align-items:start",
        "width:2ch;height:16px;flex:none;align-self:end",
        1,
        Size { cols: 20, rows: 8 },
    );
    assert_eq!(
        box_for(&overridden.tree, overridden.items[0])
            .border_rect
            .row,
        3
    );
}

#[test]
fn wrap_reverse_places_later_lines_toward_cross_start() {
    let fixture = flex_fixture(
        "width:5ch;height:48px;flex-wrap:wrap-reverse;gap:16px 1ch",
        "width:2ch;height:16px;flex:none",
        3,
        Size { cols: 20, rows: 8 },
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[0]).border_rect.row, 2);
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.row, 2);
    assert_eq!(box_for(&fixture.tree, fixture.items[2]).border_rect.row, 0);
}

#[test]
fn percentage_gap_resolves_against_the_container() {
    let fixture = flex_fixture(
        "width:10ch;column-gap:10%",
        "width:2ch;height:16px;flex:none",
        2,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.col, 3);
}

#[test]
fn column_wrapping_uses_vertical_main_gap_and_horizontal_cross_gap() {
    for (direction, expected) in [
        ("column", [(0, 0), (0, 2), (4, 0)]),
        ("column-reverse", [(0, 2), (0, 0), (4, 2)]),
    ] {
        let fixture = flex_fixture(
            &format!(
                "width:10ch;height:48px;flex-direction:{direction};flex-wrap:wrap;row-gap:16px;column-gap:2ch;align-content:start;align-items:start"
            ),
            "width:2ch;height:16px;flex:none",
            3,
            Size { cols: 20, rows: 8 },
        );
        let actual: Vec<_> = fixture
            .items
            .iter()
            .map(|&node| {
                let rect = box_for(&fixture.tree, node).border_rect;
                (rect.col, rect.row)
            })
            .collect();
        assert_eq!(actual, expected, "{direction}");
    }
}

#[test]
fn align_content_positions_wrapped_lines_for_each_distribution_family() {
    for (value, expected_rows) in [
        ("start", [0, 0, 1]),
        ("normal", [0, 0, 3]),
        ("flex-start", [0, 0, 1]),
        ("center", [2, 2, 3]),
        ("end", [3, 3, 4]),
        ("flex-end", [3, 3, 4]),
        ("space-between", [0, 0, 4]),
        ("space-around", [1, 1, 3]),
        ("space-evenly", [1, 1, 3]),
        ("stretch", [0, 0, 3]),
    ] {
        let fixture = flex_fixture(
            &format!(
                "width:5ch;height:80px;flex-wrap:wrap;align-content:{value};align-items:start"
            ),
            "width:2ch;height:16px;flex:none",
            3,
            Size { cols: 20, rows: 8 },
        );
        let actual: Vec<_> = fixture
            .items
            .iter()
            .map(|&node| box_for(&fixture.tree, node).border_rect.row)
            .collect();
        assert_eq!(actual, expected_rows, "{value}");
    }
}

#[test]
fn taffy_alignment_mapping_preserves_safety_and_physical_fallbacks() {
    fn mapped(declarations: &str) -> taffy::style::Style {
        let mut document = Document::new();
        let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(&format!("main {{ display:flex;{declarations} }}"));
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        let flow = crate::layout::engine::flow::build_flow_tree(
            crate::layout::LayoutInput {
                document: &document,
                styles: &styles,
                forms: crate::core::form::FormState::empty(),
            },
            20,
        );
        let container = flow
            .boxes
            .iter()
            .find(|flow| flow.owner == Some(root))
            .unwrap();
        crate::layout::engine::taffy_style::taffy_style(
            crate::layout::engine::taffy_style::TaffyStyleInput {
                flow: container,
                root: false,
                viewport_width: 20,
                parent_direction: None,
                calc_values: &std::cell::RefCell::new(Vec::new()),
                styles: &styles,
            },
        )
    }

    let column = mapped(
        "flex-direction:column;justify-content:safe right;align-content:safe center;align-items:unsafe center",
    );
    assert_eq!(
        column.justify_content.unwrap().keyword,
        TaffyContentKeyword::Start
    );
    assert_eq!(column.justify_content.unwrap().safety, TaffySafety::Safe);
    assert_eq!(
        column.align_content.unwrap().keyword,
        TaffyContentKeyword::Center
    );
    assert_eq!(column.align_content.unwrap().safety, TaffySafety::Safe);
    assert_eq!(
        column.align_items.unwrap().keyword,
        TaffyItemKeyword::Center
    );
    assert_eq!(column.align_items.unwrap().safety, TaffySafety::Unsafe);

    let row = mapped("flex-direction:row;justify-content:right");
    assert_eq!(
        row.justify_content.unwrap().keyword,
        TaffyContentKeyword::End
    );
}
