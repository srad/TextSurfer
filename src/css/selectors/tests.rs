use super::*;
use crate::core::dom::{Attr, Document, ElementNs, NodeId};

fn matches(selector: &str, document: &Document, id: NodeId, state: DynamicState) -> bool {
    matches_target(selector, document, id, state, MatchTarget::Element)
}

fn matches_target(
    selector: &str,
    document: &Document,
    id: NodeId,
    state: DynamicState,
    target: MatchTarget,
) -> bool {
    matching_specificity(
        &parse(selector).expect("selector parses"),
        document,
        id,
        state,
        target,
    )
    .is_some()
}

#[test]
fn html_names_are_ascii_insensitive_but_foreign_names_are_not() {
    let mut document = Document::new();
    let html = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let svg = document.insert_element(
        None,
        "linearGradient",
        ElementNs::Svg,
        vec![Attr::plain("viewBox", "0 0 1 1")],
    );
    let state = DynamicState::default();
    assert!(matches("DIV", &document, html, state));
    assert!(matches("linearGradient[viewBox]", &document, svg, state));
    assert!(!matches("lineargradient", &document, svg, state));
    assert!(!matches("linearGradient[viewbox]", &document, svg, state));
}

#[test]
fn dynamic_pseudo_classes_parse_instead_of_invalidating_the_whole_selector_list() {
    let mut document = Document::new();
    let link = document.insert_element(
        None,
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "https://example.com/")],
    );
    let anchor = document.insert_element(None, "a", ElementNs::Html, vec![]);
    let state = DynamicState::default();
    assert!(matches("a:link", &document, link, state));
    assert!(matches("a:any-link", &document, link, state));
    assert!(!matches("a:link", &document, anchor, state));
    assert!(
        parse("p, a:hover").is_some(),
        "one dynamic class must not invalidate the whole selector list"
    );
}

#[test]
fn visited_never_matches_so_page_styling_cannot_observe_history() {
    let mut document = Document::new();
    let link = document.insert_element(
        None,
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "https://example.com/")],
    );
    assert!(parse("a:visited").is_some());
    assert!(!matches(
        "a:visited",
        &document,
        link,
        DynamicState::default()
    ));
}

#[test]
fn user_action_pseudo_classes_follow_the_injected_state() {
    let mut document = Document::new();
    let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
    assert!(!matches(
        "div:hover",
        &document,
        div,
        DynamicState::default()
    ));
    assert!(matches(
        "div:hover",
        &document,
        div,
        DynamicState {
            hover: Some(div),
            ..Default::default()
        }
    ));
    assert!(matches(
        "div:focus",
        &document,
        div,
        DynamicState {
            focus: Some(div),
            ..Default::default()
        }
    ));
}

#[test]
fn form_state_pseudo_classes_read_the_attributes() {
    let mut document = Document::new();
    let checked = document.insert_element(
        None,
        "input",
        ElementNs::Html,
        vec![Attr::plain("checked", "")],
    );
    let disabled = document.insert_element(
        None,
        "input",
        ElementNs::Html,
        vec![Attr::plain("disabled", "")],
    );
    let plain = document.insert_element(None, "input", ElementNs::Html, vec![]);
    let paragraph = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let state = DynamicState::default();
    assert!(matches("input:checked", &document, checked, state));
    assert!(!matches("input:checked", &document, plain, state));
    assert!(matches("input:disabled", &document, disabled, state));
    assert!(matches("input:enabled", &document, plain, state));
    assert!(!matches("input:enabled", &document, disabled, state));
    assert!(!matches(":enabled", &document, paragraph, state));
}
