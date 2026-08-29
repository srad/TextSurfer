//! The `style` attribute: an author-origin declaration source that arrives through the DOM rather
//! than through a sheet, and that the mirror has to carry itself.

use crate::core::dom::Attr;
use crate::core::style::{Rgb, Rgba, WhiteSpace};

use super::super::resolved;
use super::both;

/// `style="…"` on one `<div>`, with `css` as the only sheet.
fn styled(css: &str, inline: &str) -> super::Both {
    both(css, &[("div", vec![Attr::plain("style", inline)])])
}

#[test]
fn an_inline_declaration_reaches_both_engines() {
    for declared in ["pre", "nowrap", "pre-wrap"] {
        let both = styled("", &format!("white-space: {declared}"));
        both.agree(0, "white-space", |style| style.white_space);
    }
    assert_eq!(
        styled("", "white-space: pre").stylo(0).white_space,
        WhiteSpace::Pre,
        "the attribute is the only declaration, so a dropped block reads as the initial value"
    );
}

/// The style attribute outranks every author rule, whatever the selector's specificity. The custom
/// cascade spells that `u32::MAX`; Stylo spells it `LayerOrder::style_attribute()`.
#[test]
fn an_inline_declaration_beats_a_high_specificity_author_rule() {
    let both = styled("div#x.y.z { white-space: nowrap }", "white-space: pre");
    both.agree(0, "white-space", |style| style.white_space);
    assert_eq!(both.stylo(0).white_space, WhiteSpace::Pre);
}

#[test]
fn an_important_author_rule_beats_a_normal_inline_declaration() {
    let both = styled("div { white-space: nowrap !important }", "white-space: pre");
    both.agree(0, "white-space", |style| style.white_space);
    assert_eq!(both.stylo(0).white_space, WhiteSpace::NoWrap);
}

#[test]
fn an_important_inline_declaration_beats_an_important_author_rule() {
    let both = styled(
        "div { white-space: nowrap !important }",
        "white-space: pre !important",
    );
    both.agree(0, "white-space", |style| style.white_space);
    assert_eq!(both.stylo(0).white_space, WhiteSpace::Pre);
}

/// One bad declaration invalidates itself, not the block around it.
#[test]
fn a_malformed_inline_declaration_drops_alone() {
    let both = styled("", "white-space: nonsense; color: #00ff00");
    both.agree(0, "white-space", |style| style.white_space);
    both.agree(0, "color", |style| style.color);
    assert_eq!(both.stylo(0).white_space, WhiteSpace::Normal);
    assert_eq!(
        both.stylo(0).color,
        Some(Rgba::opaque(Rgb { r: 0, g: 255, b: 0 }))
    );
}

/// The block has to reach the mapper, not just the cascade: one property from each of the two
/// families S4b-1 and S4b-2 built, including a handle-valued one.
#[test]
fn inline_declarations_reach_every_mapper_family() {
    let both = styled("", "color: red; font-weight: bold");
    both.agree(0, "color", |style| style.color);
    both.agree(0, "bold", |style| style.bold);

    let both = styled("", "width: calc(50% - 1ch); padding-left: 25%");
    both.agree_resolved(0, "width", |tree, style| resolved::size(tree, style.width));
    both.agree_resolved(0, "padding-left", |tree, style| {
        resolved::padding(tree, style.padding.left)
    });
}
