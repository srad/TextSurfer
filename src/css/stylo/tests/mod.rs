use std::sync::atomic::Ordering;

use app_units::Au;
use style::context::QuirksMode;
use style::dom::{TDocument, TElement, TNode};
use stylo_dom::ElementState;

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::form::{ControlValue, FormState};
use crate::core::geom::Size;
use crate::core::style::{CellMetric, LengthAxis, Palette};

use super::device::{CellFontMetrics, device_with_metrics};
use super::dom::{StyleArena, StyleDom, StyloElement, StyloNode};
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

/// Mirror through the engine, never `StyleDom::build`, so every test here inherits the engine's
/// lock and its preferences. [`StyloEngine::mirror`] says why both matter.
fn mirror<'a>(
    engine: &StyloEngine,
    arena: &'a StyleArena<'a>,
    document: &Document,
) -> StyleDom<'a> {
    engine.mirror(arena, document)
}

fn element<'a>(dom: &StyleDom<'a>, id: NodeId) -> StyloElement<'a> {
    dom.element(id).expect("a mirrored element")
}

fn engine(author_css: &[&str]) -> StyloEngine {
    StyloEngine::new(
        CellMetric::DEFAULT,
        VIEWPORT,
        QuirksMode::NoQuirks,
        Palette::default(),
        author_css,
    )
}

/// Computed `border-top-width`, the cleanest scalar computed value Stylo exposes — a bare `Au`.
///
/// Every caller declares an explicit width. Stylo's *computed* value here is the specified length
/// — it does not zero for `border-style: none`, which is a used-value concern the layout applies —
/// so an element with no declaration reads as `medium`, 3px, rather than 0.
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
    let engine = engine(&[]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
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
    let engine = engine(&[]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert!(paragraph.primary_style().is_some());
}

#[test]
fn the_user_agent_sheet_reaches_the_cascade() {
    let (document, ids) = document(&[("p", vec![]), ("span", vec![])]);
    let engine = engine(&[]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);

    let mut head_document = Document::new();
    let html = head_document.insert_element(None, "html", ElementNs::Html, vec![]);
    let head = head_document.insert_element(Some(html), "head", ElementNs::Html, vec![]);
    let head_arena = StyleArena::new();
    let head_dom = mirror(&engine, &head_arena, &head_document);
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
fn form_state_seeds_checked_enabled_and_disabled_selectors() {
    let (document, ids) = document(&[
        (
            "input",
            vec![Attr::plain("type", "checkbox"), Attr::plain("checked", "")],
        ),
        ("button", vec![]),
        ("button", vec![Attr::plain("disabled", "")]),
    ]);
    let mut forms = FormState::default();
    forms.set(ids[2], ControlValue::Checked(false));
    let engine = engine(&[
        ":checked { border-top-width: 9px }",
        ":enabled { border-right-width: 7px }",
        ":disabled { border-left-width: 5px }",
    ]);
    let arena = StyleArena::new();
    let dom = engine.mirror_with_form_state(&arena, &document, &forms);
    engine.cascade(&dom);

    let checkbox = element(&dom, ids[2]);
    let enabled = element(&dom, ids[3]);
    let disabled = element(&dom, ids[4]);
    assert_eq!(border_top(checkbox), Au::from_px(3));
    assert_eq!(
        enabled
            .primary_style()
            .expect("styled")
            .clone_border_right_width()
            .0,
        Au::from_px(7)
    );
    assert_eq!(
        disabled
            .primary_style()
            .expect("styled")
            .clone_border_left_width()
            .0,
        Au::from_px(5)
    );
}

#[test]
fn an_author_rule_beats_the_user_agent_sheet() {
    let (document, ids) = document(&[("p", vec![])]);
    let engine = engine(&["p { display: none }"]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
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
    let engine = engine(&["p { display: block } p#x { display: none }"]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
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
        Palette::default(),
        &["p { border-top-style: solid; border-top-width: 2ch }"],
        device,
    );

    let (document, ids) = document(&[("p", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
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
    let dom = mirror(&engine, &arena, &document);

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
    let dom = mirror(&engine, &arena, &document);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    assert_eq!(border_top(paragraph), Au::from_px(20));
}

/// The M7 perf measurement, reported rather than asserted.
///
/// Run with `cargo test --release measure_the_live_page_cascade -- --ignored --nocapture`. It is `#[ignore]`d
/// because a timing assertion in the standing gate rows would be flaky; the gate decision is made
/// by reading this against `benches/linux_live.rs`, which measures the same page through the custom
/// cascade. Node counts are reported alongside because a fast traversal that skipped most of the
/// tree is a different result from a fast traversal that did the work.
#[test]
#[ignore = "perf measurement, not an assertion; see benches/linux_live.rs for the other half"]
fn measure_the_live_page_cascade() {
    const PAGE: &str = include_str!("../../../../tests/fixtures/linux-live/page.html");
    const MODULES_CSS: &str = include_str!("../../../../tests/fixtures/linux-live/modules.css");
    const SITE_CSS: &str = include_str!("../../../../tests/fixtures/linux-live/site.css");

    use crate::html::HtmlParser;

    let parsed = std::time::Instant::now();
    let outcome = crate::html::Html5everParser::new(false).parse_document(PAGE);
    let document = outcome.document.borrow();
    let parse = parsed.elapsed();

    // The engine first, and not only for tidiness: it owns the lock the mirror's style attributes
    // are wrapped in, and it enables the preferences Stylo reads while parsing them.
    let started = std::time::Instant::now();
    let engine = StyloEngine::new(
        CellMetric::DEFAULT,
        Size {
            cols: 120,
            rows: 40,
        },
        QuirksMode::NoQuirks,
        Palette::default(),
        &[MODULES_CSS, SITE_CSS],
    );
    let stylist = started.elapsed();

    let built = std::time::Instant::now();
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    let build = built.elapsed();

    let cascaded = std::time::Instant::now();
    let styled = engine.cascade(&dom);
    let cascade = cascaded.elapsed();

    println!("linux-live — Stylo cascade");
    println!(
        "  html parse      : {:>7.1} ms",
        parse.as_secs_f64() * 1000.0
    );
    println!(
        "  mirror build    : {:>7.1} ms",
        build.as_secs_f64() * 1000.0
    );
    println!(
        "  stylist (parse 2 sheets + flush) : {:>7.1} ms",
        stylist.as_secs_f64() * 1000.0
    );
    println!(
        "  cascade         : {:>7.1} ms",
        cascade.as_secs_f64() * 1000.0
    );
    // S4a's gate wanted cascade + mapping under ~130 ms and the cascade alone spent 27 of it. This
    // is what tells us whether the mapper spent the rest. The distinct count is the memo working:
    // Stylo's sharing cache hands long runs of elements one `ComputedValues`, and a mapper that
    // defeated that sharing would show up here and nowhere else.
    let mapped_at = std::time::Instant::now();
    let (tree, stats) = super::map::style_tree_measured(
        &dom,
        &engine,
        &document,
        crate::core::style::RenderContext::terminal(Size {
            cols: 120,
            rows: 40,
        }),
    );
    let mapping = mapped_at.elapsed();

    println!("  mirrored nodes  : {}", dom.len());
    println!("  styled elements : {styled}");
    println!(
        "  map to cells    : {:>7.1} ms",
        mapping.as_secs_f64() * 1000.0
    );
    println!(
        "  distinct styles : {} of {} elements ({:.1}% memo hits)",
        stats.distinct,
        stats.elements,
        100.0 - (stats.distinct as f64 / stats.elements.max(1) as f64) * 100.0
    );
    println!(
        "  cascade + map   : {:>7.1} ms   (S4a gate budget: ~130 ms)",
        (cascade + mapping).as_secs_f64() * 1000.0
    );
    drop(tree);

    // The hover path: what M7 is now aimed at. The custom engine walks every element in the
    // document for this (`css/cascade/dynamic.rs:40`) and takes 105 ms on the same page.
    //
    let (_, link) = deepest_link(dom.document().as_node(), 0).expect("the page has links");
    let chain = super::invalidate::hover_chain(link);

    let changed = std::time::Instant::now();
    let change = super::invalidate::StateChange::apply(&dom, &chain, ElementState::HOVER);
    let snapshot = changed.elapsed();

    let restyled = std::time::Instant::now();
    let visited = engine.restyle(&dom, change.snapshots());
    let restyle = restyled.elapsed();

    println!();
    println!(
        "  hover chain     : {} elements (the link and its ancestors)",
        chain.len()
    );
    println!(
        "  snapshot + reset: {:>7.1} ms",
        snapshot.as_secs_f64() * 1000.0
    );
    println!(
        "  restyle         : {:>7.1} ms   (custom engine: 105 ms)",
        restyle.as_secs_f64() * 1000.0
    );
    println!("  elements visited: {visited} of {styled}");
    println!();
    println!("  NOTE: the restyle figure stops at ComputedValues — it does not re-map, which S6b");
    println!("  will. The custom engine's 105 ms also clones the whole StyleTree, so the");
    println!("  comparison is an upper bound on the custom side and a lower bound on ours.");
}

/// `<html><body><a href><span/></a> <p/> …siblings… </body></html>`
/// The most deeply nested link, ties going to the first in document order.
///
/// Deterministic, which `StyleDom::elements` is not — it iterates a `HashMap`, so an arbitrary pick
/// gives a different ancestor chain on every run and no two measurements can be compared. Deepest
/// rather than first because the ancestor chain is what a hover snapshots: the first link in a
/// document is often a skip-link three elements from the root, which would flatter the number.
fn deepest_link<'dom>(node: StyloNode<'dom>, depth: usize) -> Option<(usize, StyloElement<'dom>)> {
    let mut best = node
        .as_element()
        .filter(selectors::Element::is_link)
        .map(|element| (depth, element));
    let mut child = node.first_child();
    while let Some(node) = child {
        if let Some(found) = deepest_link(node, depth + 1)
            && best.is_none_or(|(best, _)| found.0 > best)
        {
            best = Some(found);
        }
        child = node.next_sibling();
    }
    best
}

fn hover_fixture(siblings: usize) -> (Document, NodeId, NodeId, NodeId) {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let anchor = document.insert_element(
        Some(body),
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "/somewhere")],
    );
    let inner = document.insert_element(Some(anchor), "span", ElementNs::Html, vec![]);
    let bystander = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    for _ in 0..siblings {
        document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    }
    (document, anchor, inner, bystander)
}

fn hover<'dom>(dom: &StyleDom<'dom>, engine: &StyloEngine, element: StyloElement<'dom>) -> usize {
    let chain = crate::css::stylo::invalidate::hover_chain(element);
    let change =
        crate::css::stylo::invalidate::StateChange::apply(dom, &chain, ElementState::HOVER);
    engine.restyle(dom, change.snapshots())
}

#[test]
fn a_hover_restyles_the_hovered_element_and_leaves_its_neighbours_alone() {
    let engine = engine(&[
        "a { border-top-style: solid; border-top-width: 1px }",
        "a:hover { border-top-width: 4px }",
    ]);
    let (document, anchor, _, bystander) = hover_fixture(0);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    engine.cascade(&dom);

    let anchor = element(&dom, anchor);
    let bystander = element(&dom, bystander);
    let before_anchor = anchor.primary_style().expect("styled");
    let before_bystander = bystander.primary_style().expect("styled");
    assert_eq!(border_top(anchor), Au::from_px(1), "not hovered yet");

    hover(&dom, &engine, anchor);

    let after_anchor = anchor.primary_style().expect("styled");
    let after_bystander = bystander.primary_style().expect("styled");
    assert_eq!(border_top(anchor), Au::from_px(4), "the hover rule applied");
    assert!(
        !servo_arc::Arc::ptr_eq(&before_anchor, &after_anchor),
        "the hovered element got a fresh style"
    );
    assert!(
        servo_arc::Arc::ptr_eq(&before_bystander, &after_bystander),
        "an unrelated sibling keeps the very same computed values"
    );
}

/// The invalidator has to be consulted for this: a self-only shortcut would leave the `span` alone.
#[test]
fn a_descendant_selector_hover_restyles_the_descendant() {
    let engine = engine(&[
        "span { border-top-style: solid; border-top-width: 1px }",
        "a:hover span { border-top-width: 7px }",
    ]);
    let (document, anchor, inner, _) = hover_fixture(0);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    engine.cascade(&dom);

    let anchor = element(&dom, anchor);
    let inner = element(&dom, inner);
    assert_eq!(border_top(inner), Au::from_px(1));

    hover(&dom, &engine, anchor);
    assert_eq!(border_top(inner), Au::from_px(7));
}

/// A standing M7 adoption gate: growing unrelated siblings must not grow the restyled-node count.
#[test]
fn hover_locality_does_not_degrade_as_the_document_grows() {
    let visited_for = |siblings: usize| {
        let engine = engine(&[
            "a { border-top-style: solid; border-top-width: 1px }",
            "a:hover { border-top-width: 4px }",
        ]);
        let (document, anchor, _, _) = hover_fixture(siblings);
        let arena = StyleArena::new();
        let dom = mirror(&engine, &arena, &document);
        engine.cascade(&dom);
        hover(&dom, &engine, element(&dom, anchor))
    };

    let small = visited_for(1_000);
    let large = visited_for(10_000);
    assert_eq!(
        small, large,
        "a local hover must not restyle more elements just because the document grew"
    );
}

#[test]
fn one_traversal_styles_the_whole_tree() {
    let engine = engine(&[]);
    let (document, ids) = document(&[("p", vec![]), ("span", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);

    let styled = engine.cascade(&dom);
    assert_eq!(styled, ids.len(), "every element resolves in one pass");
    for id in ids {
        assert!(element(&dom, id).primary_style().is_some());
    }
}

#[test]
fn a_traversal_over_an_empty_document_styles_nothing_rather_than_panicking() {
    let engine = engine(&[]);
    let document = Document::new();
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);

    // `traverse_dom` panics on a token that says not to traverse, so the entry point has to check.
    assert_eq!(engine.cascade(&dom), 0);
}

#[test]
fn a_traversal_applies_author_rules_across_the_tree() {
    let engine = engine(&["span { display: none }"]);
    let (document, ids) = document(&[("p", vec![]), ("span", vec![])]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    engine.cascade(&dom);

    let display_none = |id| {
        element(&dom, id)
            .primary_style()
            .expect("resolved")
            .get_box()
            .clone_display()
            .is_none()
    };
    assert!(display_none(ids[3]), "<span> matched the author rule");
    assert!(!display_none(ids[2]), "<p> did not");
}

#[test]
fn resolving_an_ancestor_chain_styles_every_element_once() {
    let (document, ids) = document(&[("p", vec![])]);
    let engine = engine(&[]);
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, &document);
    let paragraph = element(&dom, ids[2]);

    engine.resolve_with_ancestors(paragraph);
    for id in ids {
        assert!(
            element(&dom, id).primary_style().is_some(),
            "every ancestor is styled top-down"
        );
    }
}
