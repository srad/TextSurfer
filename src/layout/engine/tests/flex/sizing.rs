use super::{box_for, flex_fixture};
use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};
use crate::layout::{LayoutEngine, LayoutRect, TaffyLayoutEngine};
use proptest::prelude::*;

#[test]
fn column_flex_grow_and_explicit_height_share_the_main_axis() {
    let fixture = flex_fixture(
        "flex-direction:column;height:64px",
        "flex:1 1 0",
        2,
        Size { cols: 20, rows: 8 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.items[0]).border_rect.height,
        2
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.row, 2);
    assert_eq!(
        box_for(&fixture.tree, fixture.items[1]).border_rect.height,
        2
    );
}

#[test]
fn math_flex_basis_resolves_on_the_parent_main_axis() {
    let fixture = flex_fixture(
        "width:10ch",
        "height:16px;flex:0 0 calc(10% + 1ch)",
        2,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.items[0]).border_rect.width,
        2
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.col, 2);
}

#[test]
fn flex_grow_distributes_free_space_by_factor() {
    let equal = flex_fixture(
        "width:12ch",
        "height:16px;flex:1 1 0",
        2,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(box_for(&equal.tree, equal.items[0]).border_rect.width, 6);
    assert_eq!(box_for(&equal.tree, equal.items[1]).border_rect.width, 6);

    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "a");
    let second = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "b");
    let sheet = CssparserParser.parse(
        "main { display:flex;width:12ch;margin:0 } div { height:16px;margin:0;padding:0;flex-basis:0 } div:first-child { flex-grow:1 } div:last-child { flex-grow:2 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(box_for(&tree, first).border_rect.width, 4);
    assert_eq!(box_for(&tree, second).border_rect.width, 8);
}

#[test]
fn flex_shrink_distributes_negative_space() {
    let fixture = flex_fixture(
        "width:6ch",
        "width:4ch;height:16px;flex:0 1 auto",
        2,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.items[0]).border_rect.width,
        3
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.items[1]).border_rect.width,
        3
    );
}

#[test]
fn fixed_min_and_max_width_constrain_flex_items() {
    let fixture = flex_fixture(
        "width:12ch",
        "width:8ch;min-width:4ch;max-width:6ch;height:16px;flex:none",
        1,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.items[0]).border_rect.width,
        6
    );
}

#[test]
fn auto_main_axis_margins_share_remaining_space() {
    let fixture = flex_fixture(
        "width:10ch",
        "width:2ch;height:16px;flex:none;margin-left:auto",
        2,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(box_for(&fixture.tree, fixture.items[0]).border_rect.col, 3);
    assert_eq!(box_for(&fixture.tree, fixture.items[1]).border_rect.col, 8);
}

#[test]
fn flex_basis_resolves_length_percentage_and_content_on_the_main_axis() {
    let row = flex_fixture(
        "width:10ch",
        "height:16px;flex:0 0 32px",
        1,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(box_for(&row.tree, row.items[0]).border_rect.width, 4);

    let column = flex_fixture(
        "height:160px;flex-direction:column",
        "width:2ch;flex:0 0 32px",
        1,
        Size { cols: 20, rows: 12 },
    );
    assert_eq!(box_for(&column.tree, column.items[0]).border_rect.height, 2);

    let percentage = flex_fixture(
        "width:10ch",
        "height:16px;flex:0 0 50%",
        1,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&percentage.tree, percentage.items[0])
            .border_rect
            .width,
        5
    );

    let content = flex_fixture(
        "width:20ch",
        "height:16px;flex:0 0 content",
        1,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&content.tree, content.items[0]).border_rect.width,
        1
    );
}

#[test]
fn percentage_min_and_max_width_constrain_flex_items() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let minimum = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(minimum), "m");
    let maximum = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(maximum), "x");
    let sheet = CssparserParser.parse(
        "main { display:flex;width:30ch;margin:0 } main > div { height:16px;margin:0;flex:none } main > div:first-child { width:2ch;min-width:20% } main > div:last-child { width:8ch;max-width:20% }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 5 });
    assert_eq!(box_for(&tree, minimum).border_rect.width, 6);
    assert_eq!(box_for(&tree, maximum).border_rect.width, 6);
}

#[test]
fn box_sizing_controls_fixed_flex_item_border_geometry() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let content_box = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(content_box), "c");
    let border_box = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(border_box), "b");
    let sheet = CssparserParser.parse(
        "main { display:flex;width:30ch;margin:0 } main > div { height:16px;margin:0;flex:none;width:4ch;padding:0 1ch;border:solid } main > div:first-child { box-sizing:content-box } main > div:last-child { box-sizing:border-box }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 5 });
    assert_eq!(box_for(&tree, content_box).border_rect.width, 8);
    assert_eq!(box_for(&tree, border_box).border_rect.width, 4);
}

#[test]
fn an_empty_flex_container_keeps_its_explicit_size() {
    let fixture = flex_fixture(
        "width:5ch;height:32px",
        "flex:none",
        0,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.container).border_rect,
        LayoutRect {
            col: 0,
            row: 0,
            width: 5,
            height: 2
        }
    );
    assert!(fixture.tree.fragments.is_empty());
}

#[test]
fn a_fixed_item_overflows_a_zero_sized_flex_container_without_corrupting_geometry() {
    let fixture = flex_fixture(
        "width:0;height:0",
        "width:2ch;height:16px;flex:none",
        1,
        Size { cols: 20, rows: 5 },
    );
    assert_eq!(
        box_for(&fixture.tree, fixture.container).border_rect,
        LayoutRect {
            col: 0,
            row: 0,
            width: 0,
            height: 0
        }
    );
    let child = box_for(&fixture.tree, fixture.items[0]).border_rect;
    assert_eq!(
        (child.col, child.row, child.width, child.height),
        (0, 0, 2, 1)
    );
}

proptest! {
    #[test]
    fn wider_flex_viewports_never_increase_wrapped_height(
        item_count in 1usize..24,
        item_width in 1usize..10,
        narrow in 1u16..40,
        extra in 1u16..40,
    ) {
        let mut document = Document::new();
        let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
        for _ in 0..item_count {
            let item = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
            document.insert_text(Some(item), "x");
        }
        let sheet = CssparserParser.parse(&format!(
            "main {{ display:flex;flex-wrap:wrap;gap:16px 1ch;margin:0 }} main > div {{ width:{item_width}ch;height:16px;flex:none;margin:0 }}"
        ));
        let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
        let narrow_tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: narrow, rows: 8 },
        );
        let wide_tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: narrow + extra, rows: 8 },
        );
        prop_assert!(wide_tree.height <= narrow_tree.height);
    }
}
