use super::*;
use crate::core::dom::{Attr, Document, DomQuirksMode, ElementNs, NodeId};

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
    assert!(parse("p::before:focus-visible").is_some());
    assert!(parse("p::after:focus-within").is_some());
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
            focus: Some(FocusedNode {
                node: div,
                source: FocusSource::Pointer,
            }),
            ..Default::default()
        }
    ));
}

#[test]
fn hover_and_active_match_the_target_ancestor_chain() {
    let mut document = Document::new();
    let link = document.insert_element(None, "a", ElementNs::Html, vec![Attr::plain("href", "/")]);
    let span = document.insert_element(Some(link), "span", ElementNs::Html, vec![]);
    let text = document.insert_text(Some(span), "target");
    let state = DynamicState {
        hover: Some(text),
        active: Some(text),
        ..Default::default()
    };
    assert!(matches("a:hover span", &document, span, state));
    assert!(matches("a:active", &document, link, state));
    assert!(matches(
        ":not(:hover)",
        &document,
        span,
        DynamicState::INERT
    ));
}

#[test]
fn focus_variants_keep_exact_within_and_visible_semantics() {
    let mut document = Document::new();
    let form = document.insert_element(None, "form", ElementNs::Html, vec![]);
    let input = document.insert_element(Some(form), "input", ElementNs::Html, vec![]);
    let pointer = DynamicState {
        focus: Some(FocusedNode {
            node: input,
            source: FocusSource::Pointer,
        }),
        ..Default::default()
    };
    assert!(!matches(":focus", &document, form, pointer));
    assert!(matches(":focus-within", &document, form, pointer));
    assert!(matches(":focus-visible", &document, input, pointer));

    let button = document.insert_element(Some(form), "button", ElementNs::Html, vec![]);
    let pointer = DynamicState {
        focus: Some(FocusedNode {
            node: button,
            source: FocusSource::Pointer,
        }),
        ..Default::default()
    };
    let keyboard = DynamicState {
        focus: Some(FocusedNode {
            node: button,
            source: FocusSource::Keyboard,
        }),
        ..Default::default()
    };
    assert!(!matches(":focus-visible", &document, button, pointer));
    assert!(matches(":focus-visible", &document, button, keyboard));
}

#[test]
fn dependency_analysis_visits_nested_and_non_rightmost_components() {
    let selectors = parse("section:is(.note, a:hover) button:focus-within span").unwrap();
    assert_eq!(
        uses_dynamic_state(&selectors),
        StateDeps {
            hover: true,
            focus: true,
            active: false,
        }
    );
    let selectors = parse("button:active > span").unwrap();
    assert_eq!(
        uses_dynamic_state(&selectors),
        StateDeps {
            active: true,
            ..Default::default()
        }
    );
}

#[test]
fn quirks_mode_suppresses_hover_and_active_on_non_links() {
    let mut document = Document::new();
    document.set_quirks_mode(DomQuirksMode::Quirks);
    let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let state = DynamicState {
        hover: Some(div),
        active: Some(div),
        ..Default::default()
    };
    assert!(!matches(":hover", &document, div, state));
    assert!(!matches(":active", &document, div, state));
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
