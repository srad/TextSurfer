use std::sync::atomic::Ordering;

use app_units::Au;
use style::context::QuirksMode;
use style::dom::TElement;

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::geom::Size;
use crate::core::style::{CellMetric, LengthAxis};

use super::device::{CellFontMetrics, device_with_metrics};
use super::dom::{StyleArena, StyleDom, StyloElement};
use super::engine::{StyloEngine, mark_layout_thread};

const VIEWPORT: Size = Size { cols: 80, rows: 24 };

fn document(body: &[(&str, Vec<Attr>)]) -> (Document, Vec<NodeId>) {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body_id = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let mut ids = vec![html, body_id];
    for (name, attrs) in body {
        ids.push(document.insert_element(Some(body_id), name, ElementNs::Html, attrs.clone()));
    }
    (document, ids)
}

fn mirror<'a>(arena: &'a StyleArena<'a>, document: &Document) -> StyleDom<'a> {
    StyleDom::build(arena, document, style::shared_lock::SharedRwLock::new())
}

fn element<'a>(dom: &StyleDom<'a>, id: NodeId) -> StyloElement<'a> {
    dom.element(id).expect("a mirrored element")
}

fn engine(author_css: &[&str]) -> StyloEngine {
    StyloEngine::new(
        CellMetric::DEFAULT,
        VIEWPORT,
        QuirksMode::NoQuirks,
        author_css,
    )
}

/// Computed `border-top-width`, the cleanest scalar computed value Stylo exposes — a bare `Au`.
/// Every caller pairs it with `border-top-style: solid`, because a `none` border style computes
/// its width to zero and would mask whatever the length actually resolved to.
fn border_top(element: StyloElement<'_>) -> Au {
    element
        .primary_style()
        .expect("the element was resolved")
        .clone_border_top_width()
        .0
}

#[test]
fn element_data_is_allocated_lazily() {
    mark_layout_thread();
    let (document, ids) = document(&[]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let html = element(&dom, ids[0]);

    assert!(!TElement::has_data(&html));
    assert!(TElement::borrow_data(&html).is_none());
    assert!(TElement::mutate_data(&html).is_none());

    html.allocate_data();
    assert!(TElement::has_data(&html));
    assert!(TElement::borrow_data(&html).is_some());

    html.release_data();
    assert!(!TElement::has_data(&html));
    assert!(TElement::borrow_data(&html).is_none());
}

/// Stylo reads a parent's style straight out of the parent's `ElementData` with an `unwrap`, so an
/// adapter that claims data before it has any turns "resolve a child first" into a panic.
#[test]
fn resolving_a_child_whose_parent_is_unstyled_does_not_panic() {
    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let engine = engine(&[]);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert!(paragraph.primary_style().is_some());
}

#[test]
fn the_user_agent_sheet_reaches_the_cascade() {
    let (document, ids) = document(&[("p", vec![]), ("span", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let engine = engine(&[]);

    let mut head_document = Document::new();
    let html = head_document.insert_element(None, "html", ElementNs::Html, vec![]);
    let head = head_document.insert_element(Some(html), "head", ElementNs::Html, vec![]);
    let head_arena = StyleArena::new();
    let head_dom = mirror(&head_arena, &head_document);
    let head_element = element(&head_dom, head);
    engine.resolve_with_ancestors(head_element);
    assert!(
        head_element
            .primary_style()
            .expect("resolved")
            .get_box()
            .clone_display()
            .is_none(),
        "the UA sheet hides <head>"
    );

    for id in [ids[2], ids[3]] {
        let styled = element(&dom, id);
        engine.resolve_with_ancestors(styled);
        assert!(
            !styled
                .primary_style()
                .expect("resolved")
                .get_box()
                .clone_display()
                .is_none(),
            "the UA sheet leaves ordinary content visible"
        );
    }
}

#[test]
fn an_author_rule_beats_the_user_agent_sheet() {
    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let engine = engine(&["p { display: none }"]);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert!(
        paragraph
            .primary_style()
            .expect("resolved")
            .get_box()
            .clone_display()
            .is_none()
    );
}

#[test]
fn higher_specificity_wins_within_the_author_origin() {
    let (document, ids) = document(&[("p", vec![Attr::plain("id", "x")])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let engine = engine(&["p { display: block } p#x { display: none }"]);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert!(
        paragraph
            .primary_style()
            .expect("resolved")
            .get_box()
            .clone_display()
            .is_none()
    );
}

/// `ex` and `ch` must stay at half the font size, which is the locked terminal rule in
/// `CellMetric::css_pixels_with_fonts`. Stylo's own spec fallback happens to agree, so the value
/// alone proves nothing — the counter is what shows the provider was actually consulted.
#[test]
fn ex_and_ch_resolve_to_half_the_font_size_through_the_provider() {
    mark_layout_thread();
    let metrics = CellFontMetrics::new(CellMetric::DEFAULT);
    let counter = metrics.counter();
    let device = device_with_metrics(
        CellMetric::DEFAULT,
        VIEWPORT,
        QuirksMode::NoQuirks,
        Box::new(metrics),
    );
    let engine = StyloEngine::with_metrics(
        VIEWPORT,
        QuirksMode::NoQuirks,
        &["p { border-top-style: solid; border-top-width: 2ch }"],
        device,
    );

    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, ids[2]);
    engine.resolve_with_ancestors(paragraph);

    // 2ch at the initial 16px font size is 2 × 8px.
    assert_eq!(border_top(paragraph), Au::from_px(16));
    assert!(
        counter.load(Ordering::Relaxed) > 0,
        "the cell font metrics provider must be consulted, not bypassed for the spec fallback"
    );
}

#[test]
fn ex_and_ch_track_the_font_size_rather_than_the_cell_width() {
    let engine = engine(&[
        "p { font-size: 32px; border-top-style: solid; border-top-width: 1ch }",
        "span { font-size: 32px; border-top-style: solid; border-top-width: 1ex }",
    ]);
    let (document, ids) = document(&[("p", vec![]), ("span", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);

    for id in [ids[2], ids[3]] {
        let styled = element(&dom, id);
        engine.resolve_with_ancestors(styled);
        // Half of 32px, not the 8px cell width.
        assert_eq!(border_top(styled), Au::from_px(16));
    }
}

#[test]
fn the_device_viewport_matches_the_shared_cell_arithmetic() {
    mark_layout_thread();
    let metric = CellMetric::new(10, 20, 18).expect("a valid metric");
    let viewport = Size { cols: 42, rows: 13 };
    let device = device_with_metrics(
        metric,
        viewport,
        QuirksMode::NoQuirks,
        Box::new(CellFontMetrics::new(metric)),
    );

    let size = device.viewport_size();
    assert_eq!(
        f64::from(size.width),
        metric.viewport_css_pixels(LengthAxis::Horizontal, viewport)
    );
    assert_eq!(
        f64::from(size.height),
        metric.viewport_css_pixels(LengthAxis::Vertical, viewport)
    );
}

/// `em` is the test that proves inheritance actually reached the resolver: it can only produce the
/// right answer if the ancestor chain was resolved first and its font size was read back.
#[test]
fn em_resolves_against_the_inherited_font_size() {
    let engine = engine(&[
        "body { font-size: 40px }",
        "p { border-top-style: solid; border-top-width: 0.5em }",
    ]);
    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert_eq!(border_top(paragraph), Au::from_px(20));
}

#[test]
fn resolving_an_ancestor_chain_styles_every_element_once() {
    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&arena, &document);
    let engine = engine(&[]);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    for id in ids {
        assert!(
            element(&dom, id).primary_style().is_some(),
            "every ancestor is styled top-down"
        );
    }
}
