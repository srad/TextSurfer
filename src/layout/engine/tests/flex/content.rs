use super::box_for;
use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::core::style::{FlexDirection, TextRendering};
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};
use crate::layout::{LayoutEngine, LayoutRect, TaffyLayoutEngine};

#[test]
fn flex_items_use_order_modified_layout_without_changing_document_order() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "one");
    let second = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "two");
    let sheet = CssparserParser.parse(
        "main { display:flex } main > div { width:4ch;flex:none } main > div:last-child { order:-1 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(box_for(&tree, second).border_rect.col, 0);
    assert_eq!(box_for(&tree, first).border_rect.col, 4);
}

#[test]
fn inline_flex_shrinks_to_its_items_and_keeps_following_text_on_the_line() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let before = document.insert_text(Some(p), "L");
    let badge = document.insert_element(Some(p), "span", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(badge), "i", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "a");
    let second = document.insert_element(Some(badge), "i", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "b");
    let after = document.insert_text(Some(p), "R");
    let sheet =
        CssparserParser.parse("span { display:inline-flex } span > i { width:2ch;flex:none }");
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    let fragment = |node| {
        tree.fragments
            .iter()
            .find(|fragment| fragment.node == node)
            .unwrap()
    };
    assert_eq!(fragment(before).col, 0);
    assert_eq!(
        box_for(&tree, badge).border_rect,
        LayoutRect {
            col: 1,
            row: 0,
            width: 4,
            height: 1
        }
    );
    assert_eq!(box_for(&tree, first).border_rect.col, 1);
    assert_eq!(box_for(&tree, second).border_rect.col, 3);
    assert_eq!(fragment(after).col, 5);
}

#[test]
fn generated_flex_items_participate_in_order_modified_layout() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(child), "x");
    let sheet = CssparserParser.parse(
        "main { display:flex } main::before { content:'B';order:2 } main::after { content:'A';order:-1 } main > div { width:2ch;flex:none }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    let generated = |text| {
        tree.fragments
            .iter()
            .find(|fragment| fragment.text == text)
            .unwrap()
    };
    assert_eq!(generated("A").col, 0);
    assert_eq!(box_for(&tree, child).border_rect.col, 1);
    assert_eq!(generated("B").col, 3);
}

#[test]
fn flex_baseline_alignment_uses_text_baselines() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let large = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(large), "L");
    let small = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(small), "s");
    let sheet = CssparserParser.parse(
        "main { display:flex;align-items:baseline } main > div:first-child { font-size:32px }",
    );
    let styles = StyloCascade.apply(
        &[sheet],
        &document,
        MediaContext::screen().with_text_rendering(TextRendering::ScaledBitmap),
    );
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(box_for(&tree, large).border_rect.row, 0);
    assert_eq!(box_for(&tree, large).border_rect.height, 4);
    assert_eq!(box_for(&tree, small).border_rect.row, 3);
}

#[test]
fn nested_flex_and_table_items_share_order_modified_layout() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let nested = document.insert_element(Some(outer), "section", ElementNs::Html, vec![]);
    let upper = document.insert_element(Some(nested), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(upper), "up");
    let lower = document.insert_element(Some(nested), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(lower), "down");
    let table = document.insert_element(Some(outer), "table", ElementNs::Html, vec![]);
    let row = document.insert_element(Some(table), "tr", ElementNs::Html, vec![]);
    let cell = document.insert_element(Some(row), "td", ElementNs::Html, vec![]);
    document.insert_text(Some(cell), "cell");
    let sheet = CssparserParser.parse(
        "main { display:flex;width:12ch;margin:0 } section { display:flex;flex-direction:column;width:4ch;flex:none } section div { height:16px;margin:0;flex:none } table { width:4ch;margin:0;border-spacing:0;order:-1 } td { padding:0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(nested).flex.direction, FlexDirection::Column);
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 8 });
    assert_eq!(
        (
            box_for(&tree, table).border_rect,
            box_for(&tree, nested).border_rect,
            box_for(&tree, upper).border_rect,
            box_for(&tree, lower).border_rect,
        ),
        (
            LayoutRect {
                col: 0,
                row: 0,
                width: 4,
                height: 1
            },
            LayoutRect {
                col: 4,
                row: 0,
                width: 4,
                height: 2
            },
            LayoutRect {
                col: 4,
                row: 0,
                width: 4,
                height: 1
            },
            LayoutRect {
                col: 4,
                row: 1,
                width: 4,
                height: 1
            },
        )
    );
}

#[test]
fn wrapped_inline_flex_keeps_margins_size_and_first_line_baseline() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let before = document.insert_text(Some(p), "L");
    let badge = document.insert_element(Some(p), "span", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(badge), "b", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "one");
    let second = document.insert_element(Some(badge), "b", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "two");
    let after = document.insert_text(Some(p), "R");
    let sheet = CssparserParser.parse(
        "span { display:inline-flex;flex-wrap:wrap;width:4ch;margin:0 1ch } span > b { width:3ch;height:16px;flex:none }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 6 });
    let fragment = |node| {
        tree.fragments
            .iter()
            .find(|fragment| fragment.node == node)
            .unwrap()
    };
    assert_eq!((fragment(before).col, fragment(before).row), (0, 0));
    assert_eq!(
        box_for(&tree, badge).border_rect,
        LayoutRect {
            col: 2,
            row: 0,
            width: 4,
            height: 2
        }
    );
    assert_eq!(box_for(&tree, first).border_rect.row, 0);
    assert_eq!(box_for(&tree, second).border_rect.row, 1);
    assert_eq!((fragment(after).col, fragment(after).row), (7, 0));
}

#[test]
fn equal_order_values_remain_stable() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "first");
    let second = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "second");
    let early = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(early), "early");
    let sheet = CssparserParser.parse(
        "main { display:flex;width:12ch;margin:0 } main > div { width:4ch;flex:none;order:1 } main > div:last-child { order:-1 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(box_for(&tree, early).border_rect.col, 0);
    assert_eq!(box_for(&tree, first).border_rect.col, 4);
    assert_eq!(box_for(&tree, second).border_rect.col, 8);
}

#[test]
fn deeply_nested_flex_preserves_content() {
    let mut document = Document::new();
    let mut parent = document.insert_element(None, "main", ElementNs::Html, vec![]);
    for _ in 0..32 {
        parent = document.insert_element(Some(parent), "div", ElementNs::Html, vec![]);
    }
    let text = document.insert_text(Some(parent), "deep");
    let sheet = CssparserParser
        .parse("main, div { display:flex;flex-direction:column;margin:0;min-width:1ch }");
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert!(tree.fragments.iter().any(|fragment| fragment.node == text));
}
