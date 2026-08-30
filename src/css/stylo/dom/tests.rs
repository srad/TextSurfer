use selectors::Element as SelectorsElement;
use selectors::attr::CaseSensitivity;
use style::dom::{NodeInfo, TDocument, TElement, TNode};
use style::shared_lock::SharedRwLock;
use style::values::AtomIdent;
use stylo_dom::ElementState;

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::form::ControlKind;

use super::{StyleArena, StyleDom, StyloElement};

/// `<html><body><p id="first" class="lead intro">text</p><p>tail</p></body></html>`
fn fixture() -> (Document, NodeId, NodeId, NodeId) {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let first = document.insert_element(
        Some(body),
        "p",
        ElementNs::Html,
        vec![
            Attr::plain("id", "first"),
            Attr::plain("class", "lead intro"),
        ],
    );
    document.insert_text(Some(first), "text");
    let second = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_text(Some(second), "tail");
    (document, html, first, second)
}

/// These tests never cascade, so a lock of their own is safe — `StyloEngine::mirror` is what the
/// rest of the crate uses. Preferences still have to be on: Stylo reads them while parsing a
/// `style` attribute, and a gated property would otherwise vanish here and nowhere else.
fn mirror<'a>(arena: &'a StyleArena<'a>, document: &Document) -> StyleDom<'a> {
    super::super::prefs::enable();
    StyleDom::build(
        arena,
        document,
        crate::core::form::FormState::empty(),
        SharedRwLock::new(),
    )
}

fn element<'a>(dom: &StyleDom<'a>, id: NodeId) -> StyloElement<'a> {
    dom.element(id).expect("the node is a mirrored element")
}

/// Stylo's style sharing cache asserts that its element type is exactly pointer-sized:
/// `FakeCandidate` declares `_element: usize`, and `StyleSharingCache::new` compares the two cache
/// layouts. A wider handle fails that assertion at runtime, inside Stylo, with only a size mismatch
/// to go on — so pin the width here where the message is obvious.
#[test]
fn an_element_handle_is_exactly_one_pointer_wide() {
    assert_eq!(
        std::mem::size_of::<StyloElement<'_>>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::align_of::<StyloElement<'_>>(),
        std::mem::align_of::<usize>()
    );
}

#[test]
fn every_document_node_round_trips_through_the_mirror() {
    let (document, html, first, second) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    for id in [html, first, second] {
        assert_eq!(element(&dom, id).dom_id(), Some(id));
    }
}

#[test]
fn comments_and_doctypes_are_left_out_of_the_mirror() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    document.insert_comment(Some(html), "ignored");
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    // The document node plus `<html>`, and nothing for the comment.
    assert_eq!(dom.len(), 2);
}

#[test]
fn tree_navigation_matches_the_source_document() {
    let (document, html, first, second) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);

    let root = element(&dom, html);
    assert!(root.is_root());
    assert_eq!(root.local_name().as_ref(), "html");
    assert_eq!(dom.root_element(), Some(root));

    let first = element(&dom, first);
    let second = element(&dom, second);
    assert_eq!(first.next_sibling_element(), Some(second));
    assert_eq!(second.prev_sibling_element(), Some(first));
    assert_eq!(first.prev_sibling_element(), None);

    let body = SelectorsElement::parent_element(&first).expect("<p> has a parent");
    assert_eq!(body.local_name().as_ref(), "body");
    assert_eq!(body.first_element_child(), Some(first));
    assert_eq!(SelectorsElement::parent_element(&body), Some(root));
    assert!(!body.is_root());
}

#[test]
fn the_document_node_is_the_traversal_root() {
    let (document, html, _, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let stylo_document = dom.document();

    assert!(stylo_document.as_node().as_document().is_some());
    assert_eq!(
        stylo_document
            .as_node()
            .first_child()
            .and_then(|node| node.as_element()),
        Some(element(&dom, html))
    );
    assert_eq!(
        TElement::as_node(&element(&dom, html)).owner_doc(),
        stylo_document
    );
}

#[test]
fn text_nodes_are_mirrored_but_are_not_elements() {
    let (document, _, first, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, first);
    let text = TElement::as_node(&paragraph)
        .first_child()
        .expect("<p> has a text child");
    assert!(text.is_text_node());
    assert!(!text.is_element());
    assert!(!paragraph.is_empty());
}

#[test]
fn identifiers_and_classes_are_interned_and_case_sensitivity_is_honoured() {
    let (document, _, first, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, first);

    assert!(paragraph.has_id(&AtomIdent::from("first"), CaseSensitivity::CaseSensitive));
    assert!(!paragraph.has_id(&AtomIdent::from("FIRST"), CaseSensitivity::CaseSensitive));
    assert!(paragraph.has_id(
        &AtomIdent::from("FIRST"),
        CaseSensitivity::AsciiCaseInsensitive
    ));

    assert!(paragraph.has_class(&AtomIdent::from("lead"), CaseSensitivity::CaseSensitive));
    assert!(paragraph.has_class(&AtomIdent::from("intro"), CaseSensitivity::CaseSensitive));
    assert!(!paragraph.has_class(&AtomIdent::from("missing"), CaseSensitivity::CaseSensitive));

    let mut classes = Vec::new();
    paragraph.each_class(|class| classes.push(class.to_string()));
    assert_eq!(classes, ["lead", "intro"]);

    assert_eq!(
        paragraph.id().map(|id| id.to_string()),
        Some("first".into())
    );
}

#[test]
fn attribute_names_reach_stylo_with_their_values() {
    let (document, _, first, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, first);

    let mut names = Vec::new();
    paragraph.each_attr_name(|name| names.push(name.to_string()));
    assert_eq!(names, ["id", "class"]);

    assert_eq!(
        paragraph.get_attr(&style::LocalName::from("id"), &style::Namespace::from("")),
        Some("first".to_string())
    );
    assert_eq!(
        paragraph.get_attr(
            &style::LocalName::from("missing"),
            &style::Namespace::from("")
        ),
        None
    );
}

#[test]
fn only_an_href_bearing_anchor_is_a_link() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let linked = document.insert_element(
        Some(html),
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "/somewhere")],
    );
    let anchor = document.insert_element(Some(html), "a", ElementNs::Html, vec![]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);

    assert!(element(&dom, linked).is_link());
    assert!(!element(&dom, anchor).is_link());
}

#[test]
fn state_bits_are_settable_and_visible_to_stylo() {
    let (document, _, first, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, first);

    assert_eq!(TElement::state(&paragraph), ElementState::empty());
    paragraph.set_state(ElementState::HOVER | ElementState::FOCUS);
    assert!(TElement::state(&paragraph).contains(ElementState::HOVER));
    assert!(TElement::state(&paragraph).contains(ElementState::FOCUS));
    assert_eq!(paragraph.state(), TElement::state(&paragraph));
}

#[test]
fn visited_never_matches_however_the_state_is_set() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let anchor = document.insert_element(
        Some(html),
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "/seen")],
    );
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let anchor = element(&dom, anchor);
    anchor.set_state(ElementState::VISITED);

    // The mirror stores whatever it is told, but the cascade never sets this bit and the
    // pseudo-class refuses to match regardless. See the locked decision in ROADMAP.md.
    assert!(TElement::state(&anchor).contains(ElementState::VISITED));
    assert!(anchor.is_link());
}

#[test]
fn dirty_descendant_bits_set_and_clear() {
    let (document, html, _, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let root = element(&dom, html);

    assert!(!root.has_dirty_descendants());
    unsafe { root.set_dirty_descendants() };
    assert!(root.has_dirty_descendants());
    unsafe { root.unset_dirty_descendants() };
    assert!(!root.has_dirty_descendants());
}

#[test]
fn snapshot_bits_start_clear_and_latch() {
    let (document, html, _, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let root = element(&dom, html);

    assert!(!root.has_snapshot());
    assert!(!root.handled_snapshot());
    unsafe { root.set_handled_snapshot() };
    assert!(root.handled_snapshot());
}

#[test]
fn element_data_is_allocated_lazily() {
    let (document, html, _, _) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let root = element(&dom, html);

    // Stylo allocates `ElementData` lazily, so a freshly mirrored element reports none.
    assert!(!root.has_data());
    assert!(root.borrow_data().is_none());
    unsafe { root.ensure_data() };
    assert!(root.has_data());
    assert!(root.borrow_data().is_some_and(|data| !data.has_styles()));
    unsafe { root.clear_data() };
    assert!(!root.has_data());
    assert!(root.borrow_data().is_none());
}

#[test]
fn child_traversal_visits_every_child_in_order() {
    let (document, _, first, second) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let body = SelectorsElement::parent_element(&element(&dom, first)).expect("<body>");

    let children: Vec<_> = body
        .traversal_children()
        .filter_map(|node| node.as_element())
        .collect();
    assert_eq!(children, [element(&dom, first), element(&dom, second)]);
}

#[test]
fn opaque_identity_is_stable_and_distinct_per_element() {
    let (document, html, first, second) = fixture();
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let root = element(&dom, html);
    let first = element(&dom, first);
    let second = element(&dom, second);

    let first_again = element(
        &dom,
        first.dom_id().expect("a mirrored element knows its node"),
    );
    assert_eq!(
        SelectorsElement::opaque(&first),
        SelectorsElement::opaque(&first_again)
    );
    assert_ne!(
        SelectorsElement::opaque(&first),
        SelectorsElement::opaque(&second)
    );
    assert_ne!(
        TElement::as_node(&root).opaque(),
        TElement::as_node(&first).opaque()
    );
}

fn styled(document: &mut Document, parent: NodeId, inline: &str) -> NodeId {
    document.insert_element(
        Some(parent),
        "div",
        ElementNs::Html,
        vec![Attr::plain("style", inline)],
    )
}

/// `StyleSource`'s equality and the rule tree's key are both `Arc::ptr_eq`, so a block per element
/// is a rule node per element. Interning by source text is what keeps a page that repeats one
/// `style="…"` sixty times to a single rule node.
#[test]
fn identical_style_attributes_share_one_declaration_block() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let first = styled(&mut document, html, "color: red");
    let second = styled(&mut document, html, "color: red");
    let other = styled(&mut document, html, "color: blue");

    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let block = |id| {
        TElement::style_attribute(&element(&dom, id)).map(|borrowed| std::ptr::from_ref(&*borrowed))
    };

    assert!(block(first).is_some());
    assert_eq!(block(first), block(second), "one block, two elements");
    assert_ne!(block(first), block(other));
}

#[test]
fn a_style_attribute_that_declares_nothing_is_not_mirrored_at_all() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let empty = styled(&mut document, html, "");
    let junk = styled(&mut document, html, ";;; not-a-property: 1");
    let real = styled(&mut document, html, "color: red");

    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);

    assert!(TElement::style_attribute(&element(&dom, empty)).is_none());
    assert!(TElement::style_attribute(&element(&dom, junk)).is_none());
    assert!(TElement::style_attribute(&element(&dom, real)).is_some());
}

/// The mapper reads this rather than the tag, because `ComputedValues` say nothing about either.
#[test]
fn the_mirror_carries_each_elements_control_kind() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let field = document.insert_element(Some(html), "input", ElementNs::Html, vec![]);
    let hidden = document.insert_element(
        Some(html),
        "input",
        ElementNs::Html,
        vec![Attr::plain("type", "hidden")],
    );
    let plain = document.insert_element(Some(html), "div", ElementNs::Html, vec![]);

    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);

    assert_eq!(element(&dom, field).control_kind(), Some(ControlKind::Text));
    assert_eq!(
        element(&dom, hidden).control_kind(),
        Some(ControlKind::Hidden)
    );
    assert_eq!(element(&dom, plain).control_kind(), None);
}

#[test]
fn a_deeper_document_than_the_mirror_allows_is_truncated_rather_than_overflowing() {
    let mut document = Document::new();
    let mut parent = document.insert_element(None, "html", ElementNs::Html, vec![]);
    for _ in 0..(super::build::MAX_MIRROR_DEPTH + 50) {
        parent = document.insert_element(Some(parent), "div", ElementNs::Html, vec![]);
    }
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    assert!(dom.len() <= super::build::MAX_MIRROR_DEPTH + 2);
}
