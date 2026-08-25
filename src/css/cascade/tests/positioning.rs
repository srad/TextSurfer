use super::super::{BasicCascade, Cascade};
use crate::core::dom::{Document, ElementNs};
use crate::core::style::{ComputedStyle, CssInset, CssPercentage, Display, InsetEdges, Position};
use crate::css::{CssParser, CssparserParser, MediaContext};

fn computed(declarations: &str) -> ComputedStyle {
    let mut document = Document::new();
    let node = document.insert_element(None, "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(&format!("span {{ {declarations} }}"));
    BasicCascade
        .apply(&[sheet], &document, MediaContext::screen())
        .get(node)
}

#[test]
fn every_position_keyword_computes_and_absolute_values_blockify() {
    for (value, expected) in [
        ("static", Position::Static),
        ("relative", Position::Relative),
        ("absolute", Position::Absolute),
        ("fixed", Position::Fixed),
        ("sticky", Position::Sticky),
    ] {
        let style = computed(&format!("position:{value}"));
        assert_eq!(style.position, expected, "{value}");
        if expected.is_absolute() {
            assert_eq!(style.display, Display::BLOCK, "{value}");
        }
    }
}

#[test]
fn inset_shorthand_accepts_signed_lengths_percentages_and_auto() {
    assert_eq!(
        computed("position:absolute;inset:-16px 25% auto 16px").inset,
        InsetEdges {
            top: CssInset::Cells(-1),
            right: CssInset::Percent(CssPercentage::new(2_500)),
            bottom: CssInset::Auto,
            left: CssInset::Cells(2),
        }
    );
}

#[test]
fn inset_longhands_and_invalid_shorthands_are_atomic() {
    assert_eq!(
        computed("inset:8px;inset:-10% 1px;left:-24px").inset,
        InsetEdges {
            top: CssInset::Cells(1),
            right: CssInset::Cells(1),
            bottom: CssInset::Cells(1),
            left: CssInset::Cells(-3),
        }
    );
}

#[test]
fn positioning_and_insets_obey_css_wide_keywords_without_inheriting_by_default() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let sheet = CssparserParser
        .parse("div { position:relative;left:16px } span { position:inherit;left:unset }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(child).position, Position::Relative);
    assert_eq!(styles.get(child).inset.left, CssInset::Auto);
}
