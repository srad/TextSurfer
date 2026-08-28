//! `:checked`, `:enabled` and `:disabled` are host-language questions about a form control, not
//! attribute lookups on an arbitrary element.

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::form::{ControlValue, FormState};
use crate::core::style::Rgb;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};

/// `background-color`, deliberately: it does not inherit. A `color` marker made two of these cases
/// pass for the wrong reason — the disabled `<fieldset>` matched `:disabled` itself and handed the
/// colour down to descendants that had not matched anything.
const MARKER: &str = "background-color: #00ff00";
const MATCHED: Option<Rgb> = Some(Rgb::new(0, 255, 0));

/// Cascade `selector { MARKER }` over `document` and report whether `node` itself matched.
fn matches(document: &Document, node: NodeId, selector: &str) -> bool {
    let sheet = CssparserParser.parse(&format!("{selector} {{ {MARKER} }}"));
    let styles = BasicCascade.apply(&[sheet], document, MediaContext::screen());
    styles.get(node).background == MATCHED
}

fn element(
    document: &mut Document,
    parent: Option<NodeId>,
    name: &str,
    attrs: &[(&str, &str)],
) -> NodeId {
    document.insert_element(
        parent,
        name,
        ElementNs::Html,
        attrs
            .iter()
            .map(|(name, value)| Attr::plain(name, value))
            .collect(),
    )
}

#[test]
fn a_div_with_a_checked_attribute_is_not_a_checked_control() {
    // The matcher used to read the `checked` attribute off any element at all, so a `<div checked>`
    // matched `:checked` — and an author rule could then hide arbitrary content.
    let mut document = Document::new();
    let div = element(&mut document, None, "div", &[("checked", "")]);
    assert!(
        !matches(&document, div, ":checked"),
        "only a checkbox, radio or option can be checked"
    );
}

#[test]
fn only_the_toggle_input_states_and_options_can_be_checked() {
    let mut document = Document::new();
    let checkbox = element(
        &mut document,
        None,
        "input",
        &[("type", "checkbox"), ("checked", "")],
    );
    let radio = element(
        &mut document,
        None,
        "input",
        &[("type", "radio"), ("checked", "")],
    );
    // A text input carrying a stray `checked` attribute is not in a checked state.
    let text = element(
        &mut document,
        None,
        "input",
        &[("type", "text"), ("checked", "")],
    );
    let checked = ":checked";
    assert!(matches(&document, checkbox, checked));
    assert!(matches(&document, radio, checked));
    assert!(!matches(&document, text, checked));
}

#[test]
fn an_options_checkedness_is_its_selects_selection_not_its_own_attribute() {
    let mut document = Document::new();
    let select = element(&mut document, None, "select", &[]);
    let first = element(&mut document, Some(select), "option", &[]);
    let group = element(&mut document, Some(select), "optgroup", &[]);
    let second = element(&mut document, Some(group), "option", &[("selected", "")]);
    let checked = ":checked";
    assert!(
        matches(&document, second, checked),
        "the selected option is checked, through its optgroup"
    );
    assert!(
        !matches(&document, first, checked),
        "and the one it displaced is not"
    );
}

#[test]
fn checked_selector_reads_the_live_form_state() {
    let mut document = Document::new();
    let checkbox = element(&mut document, None, "input", &[("type", "checkbox")]);
    let sheet = CssparserParser.parse(&format!(":checked {{ {MARKER} }}"));
    let mut forms = FormState::default();
    forms.set(checkbox, ControlValue::Checked(true));
    let styles =
        BasicCascade.apply_with_form_state(&[sheet], &document, MediaContext::screen(), &forms);
    assert_eq!(styles.get(checkbox).background, MATCHED);
}

/// A regression guard, not a repair: the old matcher already gated `:disabled`/`:enabled` on the
/// element being a form control. The five cases around it each fail against that matcher.
#[test]
fn a_div_with_a_disabled_attribute_is_neither_enabled_nor_disabled() {
    let mut document = Document::new();
    let div = element(&mut document, None, "div", &[("disabled", "")]);
    assert!(!matches(&document, div, ":disabled"));
    assert!(
        !matches(&document, div, ":enabled"),
        "an element that cannot be disabled is not thereby enabled"
    );
}

#[test]
fn a_control_inside_a_disabled_fieldset_is_disabled() {
    let mut document = Document::new();
    let fieldset = element(&mut document, None, "fieldset", &[("disabled", "")]);
    let legend = element(&mut document, Some(fieldset), "legend", &[]);
    // The first legend of a disabled fieldset is the one carve-out HTML grants.
    let escapee = element(&mut document, Some(legend), "input", &[]);
    let wrapper = element(&mut document, Some(fieldset), "div", &[]);
    let trapped = element(&mut document, Some(wrapper), "input", &[]);
    assert!(
        matches(&document, trapped, ":disabled"),
        "a descendant of a disabled fieldset is actually disabled"
    );
    assert!(
        matches(&document, escapee, ":enabled"),
        "a control in the fieldset's first legend stays enabled"
    );
}

#[test]
fn an_option_in_a_disabled_optgroup_is_disabled() {
    let mut document = Document::new();
    let select = element(&mut document, None, "select", &[]);
    let group = element(&mut document, Some(select), "optgroup", &[("disabled", "")]);
    let option = element(&mut document, Some(group), "option", &[]);
    assert!(matches(&document, option, ":disabled"));
}
