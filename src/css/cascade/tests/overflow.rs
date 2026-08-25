use super::super::{BasicCascade, Cascade};
use crate::core::dom::{Document, ElementNs};
use crate::core::style::{ComputedStyle, Overflow, OverflowAxes, PseudoElement, Visibility};
use crate::css::{CssParser, CssparserParser, MediaContext};

fn computed(declarations: &str) -> ComputedStyle {
    let mut document = Document::new();
    let node = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(&format!("div {{ {declarations} }}"));
    BasicCascade
        .apply(&[sheet], &document, MediaContext::screen())
        .get(node)
}

#[test]
fn overflow_shorthand_axes_and_invalid_values_are_atomic() {
    assert_eq!(
        computed("overflow:hidden auto").overflow,
        OverflowAxes {
            x: Overflow::Hidden,
            y: Overflow::Auto,
        }
    );
    assert_eq!(
        computed("overflow:clip visible").overflow,
        OverflowAxes {
            x: Overflow::Clip,
            y: Overflow::Visible,
        }
    );
    assert_eq!(
        computed("overflow:visible hidden").overflow,
        OverflowAxes {
            x: Overflow::Auto,
            y: Overflow::Hidden,
        }
    );
    assert_eq!(
        computed("overflow:scroll;overflow:hidden nope").overflow,
        OverflowAxes::uniform(Overflow::Scroll)
    );
}

#[test]
fn overflow_longhands_share_the_computed_axis_fixup() {
    assert_eq!(
        computed("overflow-x:hidden").overflow,
        OverflowAxes {
            x: Overflow::Hidden,
            y: Overflow::Auto,
        }
    );
    assert_eq!(
        computed("overflow-y:clip").overflow,
        OverflowAxes {
            x: Overflow::Visible,
            y: Overflow::Clip,
        }
    );
}

#[test]
fn visibility_inherits_into_elements_and_generated_content() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser
        .parse("div { visibility:hidden } span::before { content:'x' } span { visibility:unset }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(parent).visibility, Visibility::Hidden);
    assert_eq!(styles.get(child).visibility, Visibility::Hidden);
    assert_eq!(
        styles
            .pseudo(child, PseudoElement::Before)
            .expect("generated content")
            .style
            .visibility,
        Visibility::Hidden
    );
}

#[test]
fn explicit_visible_descendant_can_reappear() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse("div { visibility:hidden } span { visibility:visible }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(child).visibility, Visibility::Visible);
}
