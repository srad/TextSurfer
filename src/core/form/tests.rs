use url::Url;

use super::*;
use crate::html::{Html5everParser, HtmlParser};

fn document(html: &str) -> crate::core::dom::SharedDocument {
    Html5everParser::new(false).parse_document(html).document
}

#[test]
fn explicit_form_owner_wins_and_invalid_reference_disowns_the_control() {
    let parsed = document(
        "<form id=first></form><form id=second><input id=owned form=first><input id=orphan form=missing></form>",
    );
    let document = parsed.borrow();
    let first = document.element_by_id("first").unwrap();
    assert_eq!(
        form_owner(&document, document.element_by_id("owned").unwrap()),
        Some(first)
    );
    assert_eq!(
        form_owner(&document, document.element_by_id("orphan").unwrap()),
        None
    );
}

#[test]
fn successful_controls_serialize_in_tree_order_with_duplicates_and_defaults() {
    let parsed = document(
        "<form id=f action='/find?old=1#results'><input name=q value='hello world'><input name=q value=again><input type=checkbox name=flag checked><input type=checkbox name=skip><input type=hidden name=_charset_ value=wrong><button name=go value=Go>Ignored label</button><input disabled name=no value=no><datalist><input name=bad value=x></datalist><select multiple name=bad><option selected>x</option></select></form>",
    );
    let document = parsed.borrow();
    let submitter = document.element_by_id("f").unwrap();
    let button = document
        .children(submitter)
        .into_iter()
        .find(|id| control_kind(&document, *id) == Some(ControlKind::Submit))
        .unwrap();
    let result = build_submission(
        &document,
        FormState::empty(),
        Some(button),
        &Url::parse("https://example.com/page").unwrap(),
        &Url::parse("https://example.com/base/").unwrap(),
    )
    .unwrap();
    assert_eq!(
        result,
        FormSubmission::Get(Url::parse("https://example.com/find?q=hello+world&q=again&flag=on&_charset_=UTF-8&go=Go#results").unwrap())
    );
}

#[test]
fn post_keeps_the_action_query_and_normalizes_line_breaks() {
    let parsed = document(
        "<form id=f method=post action='/send?token=x'><textarea name=body>a\nb\rc\r\nd</textarea><select name=pick><option>First</option><option selected value=two>Second</option></select><input id=s type=submit name=send value=yes></form>",
    );
    let document = parsed.borrow();
    let result = build_submission(
        &document,
        FormState::empty(),
        Some(document.element_by_id("s").unwrap()),
        &Url::parse("https://example.com/page").unwrap(),
        &Url::parse("https://example.com/").unwrap(),
    )
    .unwrap();
    assert_eq!(
        result,
        FormSubmission::PostUrlEncoded {
            url: Url::parse("https://example.com/send?token=x").unwrap(),
            body: b"body=a%0D%0Ab%0D%0Ac%0D%0Ad&pick=two&send=yes".to_vec(),
        }
    );
}

#[test]
fn typed_mutations_enforce_control_rules_and_radio_groups() {
    let parsed = document(
        "<form><input id=text maxlength=3><input id=readonly readonly><input id=a type=radio name=g checked><input id=b type=radio name=g><input id=c type=radio checked><input id=d type=radio><select id=pick><option disabled>no</option><option>yes</option></select></form>",
    );
    let document = parsed.borrow();
    let mut forms = FormState::default();
    forms
        .set_text(
            &document,
            document.element_by_id("text").unwrap(),
            "a😀bc".to_string(),
        )
        .unwrap();
    assert_eq!(
        text_value(&document, document.element_by_id("text").unwrap(), &forms),
        "a😀b"
    );
    assert_eq!(
        forms.set_text(
            &document,
            document.element_by_id("readonly").unwrap(),
            "x".to_string()
        ),
        Err(FormMutationError::Inert)
    );
    forms
        .set_checked(&document, document.element_by_id("b").unwrap(), true)
        .unwrap();
    assert!(!checkedness(
        &document,
        document.element_by_id("a").unwrap(),
        &forms
    ));
    assert!(checkedness(
        &document,
        document.element_by_id("b").unwrap(),
        &forms
    ));
    forms
        .set_checked(&document, document.element_by_id("d").unwrap(), true)
        .unwrap();
    assert!(checkedness(
        &document,
        document.element_by_id("c").unwrap(),
        &forms
    ));
    forms
        .select(&document, document.element_by_id("pick").unwrap(), 0)
        .unwrap();
    assert_eq!(
        selected_index(&document, document.element_by_id("pick").unwrap(), &forms),
        Some(1)
    );
}

#[test]
fn input_type_states_distinguish_unknown_values_from_known_unsupported_states() {
    for value in [
        None,
        Some(""),
        Some("text"),
        Some("SEARCH"),
        Some("unknown"),
    ] {
        assert_eq!(input_kind(value), ControlKind::Text, "{value:?}");
    }
    for value in [
        "file",
        "image",
        "color",
        "range",
        "date",
        "time",
        "month",
        "week",
        "datetime-local",
    ] {
        assert_eq!(input_kind(Some(value)), ControlKind::Unsupported, "{value}");
    }
}
