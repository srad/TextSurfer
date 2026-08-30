use crate::core::dom::{Attr, Document, ElementNs};
use crate::core::style::{Display, TextAlign};
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};

#[test]
fn stylo_cascade_passes_the_cascade_contract() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let paragraph = document.insert_element(
        Some(body),
        "p",
        ElementNs::Html,
        vec![Attr::plain("align", "right")],
    );
    let sheet = CssparserParser.parse(
        "p { display: inline } @media screen { p { display: block } } @media print { p { display: none } }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(paragraph).display, Display::BLOCK);
    assert_eq!(styles.get(paragraph).text_align, TextAlign::Right);
}
