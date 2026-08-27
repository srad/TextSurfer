use super::super::{BasicCascade, Cascade};
use crate::core::dom::{Attr, Document, ElementNs};
use crate::core::style::{Clear, CssFloat, CssMargin, Display};
use crate::css::{CssParser, CssparserParser, MediaContext};

#[test]
fn float_and_clear_parse_without_inheriting_by_default() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let inherited = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(
        "div { float:right; clear:left } span { float:none;clear:both } span + span { float:inherit;clear:unset }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(parent).float, CssFloat::None);
    assert_eq!(styles.get(parent).clear, Clear::Left);
    assert_eq!(styles.get(child).float, CssFloat::None);
    assert_eq!(styles.get(child).clear, Clear::Both);
    assert_eq!(styles.get(inherited).float, CssFloat::None);
    assert_eq!(styles.get(inherited).clear, Clear::None);
}

#[test]
fn effective_floats_blockify_but_positioned_roots_and_layout_items_ignore_float() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let floated = document.insert_element(
        Some(root),
        "span",
        ElementNs::Html,
        vec![Attr::plain("id", "float")],
    );
    let positioned = document.insert_element(
        Some(root),
        "span",
        ElementNs::Html,
        vec![Attr::plain("id", "positioned")],
    );
    let flex = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
    let item = document.insert_element(Some(flex), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(
        "main { float:left } #float { float:left } #positioned { float:right;position:absolute } section { display:flex } section span { float:left }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(root).float, CssFloat::None);
    assert_eq!(styles.get(floated).float, CssFloat::Left);
    assert_eq!(styles.get(floated).display, Display::BLOCK);
    assert_eq!(styles.get(positioned).float, CssFloat::None);
    assert_eq!(styles.get(item).float, CssFloat::None);
}

#[test]
fn invalid_float_values_do_not_replace_a_valid_winner() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let node = document.insert_element(Some(root), "span", ElementNs::Html, vec![]);
    let sheet =
        CssparserParser.parse("span { float:left;float:inline-start;clear:right;clear:all }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(node).float, CssFloat::Left);
    assert_eq!(styles.get(node).clear, Clear::Right);
}

#[test]
fn practical_float_attributes_enter_the_author_cascade() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let table = document.insert_element(
        Some(root),
        "table",
        ElementNs::Html,
        vec![Attr::plain("align", "right")],
    );
    let image = document.insert_element(
        Some(root),
        "img",
        ElementNs::Html,
        vec![
            Attr::plain("align", "left"),
            Attr::plain("hspace", "8"),
            Attr::plain("vspace", "16"),
        ],
    );
    let br = document.insert_element(
        Some(root),
        "br",
        ElementNs::Html,
        vec![Attr::plain("clear", "all")],
    );
    let sheet = CssparserParser.parse("img { float:right; margin-left:0 }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(table).float, CssFloat::Right);
    assert_eq!(styles.get(image).float, CssFloat::Right);
    assert_eq!(styles.get(image).margin.left, CssMargin::Cells(0));
    assert_eq!(styles.get(image).margin.right, CssMargin::Cells(1));
    assert_eq!(styles.get(image).margin.top, CssMargin::Cells(1));
    assert_eq!(styles.get(image).margin.bottom, CssMargin::Cells(1));
    assert_eq!(styles.get(br).clear, Clear::Both);
}
