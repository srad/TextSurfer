//! `ComputedStyle::reverse`, the one mapped value with no CSS property behind it.
//!
//! Both engines read it off the element through `core::form::control_kind`, so this is the one file
//! here that may assert on a value its own CSS does not declare — see the module comment on the
//! parent. What `reverse` means, and why it is a style bit rather than a colour, is documented on
//! `css::ua::apply_form_control`.

use crate::core::dom::{Attr, Document, ElementNs};

use super::{Both, both, custom_cascade, stylo_cascade};

fn control(tag: &str, attrs: Vec<Attr>) -> super::Both {
    both("", &[(tag, attrs)])
}

fn typed(kind: &str) -> super::Both {
    control("input", vec![Attr::plain("type", kind)])
}

#[test]
fn every_rendered_control_is_reversed() {
    for kind in [
        "text", "password", "search", "email", "checkbox", "radio", "submit", "reset", "button",
        "file", "color", "range", "date",
    ] {
        let both = typed(kind);
        both.agree(0, "reverse", |style| style.reverse);
        assert!(
            both.stylo(0).reverse,
            "<input type={kind}> renders as a control"
        );
    }
    for tag in ["textarea", "select", "button"] {
        let both = control(tag, Vec::new());
        both.agree(0, "reverse", |style| style.reverse);
        assert!(both.stylo(0).reverse, "<{tag}> renders as a control");
    }
    // A missing or unknown `type` is the text state, per the HTML input type table.
    for attrs in [Vec::new(), vec![Attr::plain("type", "wat")]] {
        let both = control("input", attrs);
        both.agree(0, "reverse", |style| style.reverse);
        assert!(both.stylo(0).reverse);
    }
}

/// A hidden input is submitted and never painted, and the three `<select>` satellites are drawn as
/// part of the select's own stand-in. None of them is a box in the flow, so none is reversed.
#[test]
fn unrendered_and_non_control_elements_are_not_reversed() {
    let both = typed("hidden");
    both.agree(0, "reverse", |style| style.reverse);
    assert!(!both.stylo(0).reverse);

    for tag in ["option", "optgroup", "datalist", "div", "form", "label"] {
        let both = control(tag, Vec::new());
        both.agree(0, "reverse", |style| style.reverse);
        assert!(!both.stylo(0).reverse, "<{tag}> is not a rendered control");
    }
}

/// `reverse` is absent from `css::ua`'s inherited-field list, so it is a bare per-element bit, not a
/// propagated one like the text decorations beside it. A `<button>`'s children are not painted, but
/// the cascade still runs over them.
#[test]
fn reverse_does_not_inherit_into_a_controls_children() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let button = document.insert_element(Some(body), "button", ElementNs::Html, vec![]);
    let label = document.insert_element(Some(button), "span", ElementNs::Html, vec![]);

    let both = Both {
        custom: custom_cascade("", &document),
        stylo: stylo_cascade("", &document),
        ids: vec![button, label],
    };

    both.agree(0, "reverse", |style| style.reverse);
    both.agree(1, "reverse", |style| style.reverse);
    assert!(both.stylo(0).reverse);
    assert!(
        !both.stylo(1).reverse,
        "the span inside it is not a control"
    );
}

/// Two inputs that differ only in `type` share one `ComputedValues` — nothing in either sheet
/// mentions the attribute, so Stylo's sharing cache hands them one allocation. `reverse` still has
/// to differ, which is why it is applied outside the mapper's memo.
#[test]
fn two_inputs_sharing_one_computed_style_still_differ_in_reverse() {
    let both = both(
        "",
        &[
            ("input", vec![Attr::plain("type", "text")]),
            ("input", vec![Attr::plain("type", "hidden")]),
        ],
    );
    both.agree(0, "reverse", |style| style.reverse);
    both.agree(1, "reverse", |style| style.reverse);
    assert!(both.stylo(0).reverse);
    assert!(!both.stylo(1).reverse);
}
