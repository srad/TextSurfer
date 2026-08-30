use super::{box_for, viewport};
use crate::core::dom::{Document, ElementNs};
use crate::core::geom::Size;
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};

#[test]
fn contiguous_text_between_boxes_becomes_its_own_anonymous_grid_item() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    document.insert_text(Some(root), "A");
    let block = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(block), "B");
    document.insert_text(Some(root), "C");
    let sheet = CssparserParser.parse(
        "main { display: grid; margin: 0; grid-template-columns: repeat(3, 2ch); width: 6ch }
         main > div { margin: 0; padding: 0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    // Three items in three columns: the two text runs are anonymous items around the real box.
    let rect = box_for(&tree, block).border_rect;
    assert_eq!((rect.col, rect.row), (2, 0));
}

#[test]
fn whitespace_between_items_does_not_generate_an_item_of_its_own() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "A");
    document.insert_text(Some(root), "   ");
    let second = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "B");
    let sheet = CssparserParser.parse(
        "main { display: grid; margin: 0; grid-template-columns: 2ch 2ch; width: 4ch }
         main > div { margin: 0; padding: 0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    assert_eq!(box_for(&tree, first).border_rect.col, 0);
    assert_eq!(box_for(&tree, second).border_rect.col, 2);
}

#[test]
fn a_table_participates_as_a_grid_item() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let table = document.insert_element(Some(root), "table", ElementNs::Html, vec![]);
    let row = document.insert_element(Some(table), "tr", ElementNs::Html, vec![]);
    let cell = document.insert_element(Some(row), "td", ElementNs::Html, vec![]);
    document.insert_text(Some(cell), "T");
    let after = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(after), "B");
    let sheet = CssparserParser.parse(
        "main { display: grid; margin: 0; grid-template-columns: 5ch 5ch; width: 10ch }
         main > * { margin: 0 }
         table { border-spacing: 0 } td { padding: 0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    assert_eq!(box_for(&tree, after).border_rect.col, 5);
}

#[test]
fn misparented_table_children_are_wrapped_and_placed_as_one_item() {
    // The anonymous-table fixup runs inside a grid container just as it does in normal flow.
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let cell = document.insert_element(Some(root), "td", ElementNs::Html, vec![]);
    document.insert_text(Some(cell), "T");
    let after = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(after), "B");
    let sheet = CssparserParser.parse(
        "main { display: grid; margin: 0; grid-template-columns: 5ch 5ch; width: 10ch }
         main > * { margin: 0 } td { padding: 0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    assert_eq!(box_for(&tree, after).border_rect.col, 5);
}

#[test]
fn a_nested_grid_lays_its_own_items_out_inside_its_area() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let nested = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(nested), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "A");
    let second = document.insert_element(Some(nested), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "B");
    let sheet = CssparserParser.parse(
        "main { display:grid; grid-template-columns:8ch; width:8ch; margin:0 }
         section { display:grid; grid-template-columns:2ch 2ch; margin:0 }
         div { margin:0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, viewport(20));
    assert_eq!(box_for(&tree, nested).border_rect.width, 8);
    assert_eq!(box_for(&tree, first).border_rect.col, 0);
    assert_eq!(box_for(&tree, second).border_rect.col, 2);
}

#[test]
fn an_inline_grid_shrinks_to_fit_and_sits_on_the_line() {
    let mut document = Document::new();
    let paragraph = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(paragraph), "before ");
    let badge = document.insert_element(Some(paragraph), "span", ElementNs::Html, vec![]);
    for text in ["A", "B"] {
        let cell = document.insert_element(Some(badge), "b", ElementNs::Html, vec![]);
        document.insert_text(Some(cell), text);
    }
    document.insert_text(Some(paragraph), " after");
    let sheet = CssparserParser.parse(
        "p { margin: 0 }
         span { display: inline-grid; grid-template-columns: 2ch 2ch }
         b { margin: 0; padding: 0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 24 });
    // The whole line fits on one row: the atom did not become a block.
    assert_eq!(tree.height, 1);
    let rect = box_for(&tree, badge).border_rect;
    assert_eq!(rect.width, 4);
}

#[test]
fn a_grid_item_that_is_a_flex_container_keeps_its_own_formatting_context() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let flex = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(flex), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(child), "A");
    let sheet = CssparserParser.parse(
        "main { display:grid; grid-template-columns:8ch; width:8ch; margin:0 }
         section { display:flex; justify-content:flex-end; margin:0 }
         div { flex:none; width:2ch; margin:0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, viewport(20));
    assert_eq!(box_for(&tree, flex).border_rect.width, 8);
    assert_eq!(box_for(&tree, child).border_rect.col, 6);
}

#[test]
fn display_contents_children_participate_as_grid_items() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let wrapper = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(wrapper), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(first), "A");
    let second = document.insert_element(Some(wrapper), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "B");
    let third = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(third), "C");
    let sheet = CssparserParser.parse(
        "main { display:grid; grid-template-columns:2ch 2ch 2ch; width:6ch; margin:0 }
         section { display:contents }
         div { margin:0 }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, viewport(20));
    assert_eq!(box_for(&tree, first).border_rect.col, 0);
    assert_eq!(box_for(&tree, second).border_rect.col, 2);
    assert_eq!(box_for(&tree, third).border_rect.col, 4);
}
