use style::context::QuirksMode;
use style::properties::ComputedValues;
use style::values::computed::LengthPercentage;
use style::values::generics::NonNegative;
use style::values::generics::length::GenericSize;

use crate::core::dom::{Document, ElementNs, NodeId};
use crate::core::geom::Size;
use crate::core::style::{CalcRange, CellMetric, LengthAxis, RenderContext, StyleStore};

use super::super::dom::{StyleArena, StyleDom};
use super::super::engine::StyloEngine;
use super::length::{Axes, Lengths, Value};
use super::style_tree_measured;

const VIEWPORT: Size = Size { cols: 80, rows: 24 };

fn context() -> RenderContext {
    RenderContext::terminal(VIEWPORT)
}

/// `<html><body><div>…</div></body></html>` with `css` as the only author sheet, cascaded.
///
/// Returns the div's computed values. Taking them out of a real cascade rather than constructing
/// them by hand is the point: a hand-built `ComputedValues` would not prove the declaration parsed.
fn computed(css: &str) -> servo_arc::Arc<ComputedValues> {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let div = document.insert_element(Some(body), "div", ElementNs::Html, vec![]);

    let engine = StyloEngine::new(CellMetric::DEFAULT, VIEWPORT, QuirksMode::NoQuirks, &[css]);
    let arena = StyleArena::new();
    let dom = StyleDom::build(&arena, &document, style::shared_lock::SharedRwLock::new());
    engine.cascade(&dom);
    dom.element(div)
        .expect("the div is mirrored")
        .primary_style()
        .expect("the div is styled")
}

/// The mapped style of a `<div>` under `css`, plus the tree whose store its handles index.
fn mapped(css: &str) -> (crate::core::style::StyleTree, NodeId) {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let div = document.insert_element(Some(body), "div", ElementNs::Html, vec![]);

    let engine = StyloEngine::new(CellMetric::DEFAULT, VIEWPORT, QuirksMode::NoQuirks, &[css]);
    let arena = StyleArena::new();
    let dom = StyleDom::build(&arena, &document, style::shared_lock::SharedRwLock::new());
    engine.cascade(&dom);
    (style_tree_measured(&dom, context()).0, div)
}

fn style_of(css: &str) -> crate::core::style::ComputedStyle {
    let (tree, div) = mapped(css);
    tree.get(div)
}

/// The declared `width` as a computed `<length-percentage>`.
fn width(css: &str) -> LengthPercentage {
    match computed(css).clone_width() {
        GenericSize::LengthPercentage(NonNegative(value)) => value,
        other => panic!("expected a <length-percentage> width, got {other:?}"),
    }
}

fn resolve(css: &str, axes: Axes, range: CalcRange) -> Option<Value> {
    let mut lengths = Lengths::new(CellMetric::DEFAULT, VIEWPORT);
    let mut store = StyleStore::default();
    lengths.resolve(&width(css), axes, range, &mut store)
}

fn cells(value: Option<Value>) -> isize {
    match value {
        Some(Value::Cells(cells)) => cells,
        other => panic!("expected cells, got {}", describe(&other)),
    }
}

fn describe(value: &Option<Value>) -> &'static str {
    match value {
        None => "nothing",
        Some(Value::Cells(_)) => "cells",
        Some(Value::Percent(_)) => "a percentage",
        Some(Value::Calc(_)) => "a calc",
    }
}

// --- preferences -----------------------------------------------------------------------------

/// Stylo gates every grid longhand and `display: grid` behind `layout.grid.enabled`, which defaults
/// to `false` and fails *silently* — the declarations are simply dropped from the rule database.
///
/// This asserts the parse, not the preference. `pref!("layout.grid.enabled") == true` would pass
/// while the cascade still saw nothing.
#[test]
fn the_grid_preference_is_on_so_grid_declarations_survive_the_cascade() {
    use style::values::computed::GridTemplateComponent;
    use style::values::specified::box_::DisplayInside;

    let values = computed("div { display: grid; grid-template-columns: 1fr 1fr }");
    assert_eq!(
        values.clone_display().inside(),
        DisplayInside::Grid,
        "`display: grid` must parse; if this fails, `css::stylo::prefs` did not run first"
    );
    match values.clone_grid_template_columns() {
        GridTemplateComponent::TrackList(list) => {
            assert_eq!(list.values.len(), 2, "both `1fr` tracks survived");
        }
        other => panic!("expected a track list, got {other:?}"),
    }
}

// --- lengths ---------------------------------------------------------------------------------

#[test]
fn absolute_lengths_quantise_to_the_axis_cell() {
    assert_eq!(
        cells(resolve(
            "div { width: 8px }",
            Axes::same(LengthAxis::Horizontal),
            CalcRange::NonNegative
        )),
        1
    );
    assert_eq!(
        cells(resolve(
            "div { width: 24px }",
            Axes::same(LengthAxis::Horizontal),
            CalcRange::NonNegative
        )),
        3
    );
    // The same pixel count is a different number of cells down the other axis: 8 columns, 16 rows.
    assert_eq!(
        cells(resolve(
            "div { width: 32px }",
            Axes::same(LengthAxis::Vertical),
            CalcRange::NonNegative
        )),
        2
    );
}

/// Stylo resolves font- and viewport-relative units itself, so the mapper only ever sees pixels —
/// which is exactly why it can be a pure quantiser.
#[test]
fn relative_units_arrive_already_resolved() {
    assert_eq!(
        cells(resolve(
            "div { width: 2em }",
            Axes::same(LengthAxis::Horizontal),
            CalcRange::NonNegative
        )),
        4,
        "2em at the initial 16px font size is 32px, four 8px columns"
    );
    assert_eq!(
        cells(resolve(
            "div { width: 10vw }",
            Axes::same(LengthAxis::Horizontal),
            CalcRange::NonNegative
        )),
        8,
        "10% of an 80-column viewport"
    );
}

#[test]
fn a_percentage_stays_a_percentage_when_both_axes_agree() {
    match resolve(
        "div { width: 50% }",
        Axes::same(LengthAxis::Horizontal),
        CalcRange::NonNegative,
    ) {
        Some(Value::Percent(fraction)) => assert_eq!(fraction, 0.5),
        other => panic!("expected a percentage, got {}", describe(&other)),
    }
}

/// A vertical margin's `50%` resolves against the containing block's *width*, so the value the
/// store holds must carry the column/row ratio. Emitting a bare percentage here would be wrong by
/// the cell aspect ratio and would not fail any type check.
#[test]
fn a_percentage_with_a_cross_axis_basis_becomes_a_scaled_calc() {
    let mut lengths = Lengths::new(CellMetric::DEFAULT, VIEWPORT);
    let mut store = StyleStore::default();
    let value = lengths.resolve(
        &width("div { width: 50% }"),
        Axes::inline_basis(LengthAxis::Vertical),
        CalcRange::Unbounded,
        &mut store,
    );
    let Some(Value::Calc(handle)) = value else {
        panic!("expected a calc, got {}", describe(&value));
    };
    // A 20-column basis is 50% → 10 columns → 5 rows, because a row is two columns tall.
    assert_eq!(store.calculations.resolve(handle, 20.0), Some(5.0));
}

/// `CalcLengthPercentage` keeps its node private, so the mapper serialises and re-parses. These are
/// the shapes that round trip: a sum wrapped in `calc(…)`, and the functions that serialise as
/// themselves.
#[test]
fn computed_calc_round_trips_through_serialisation() {
    let horizontal = Axes::same(LengthAxis::Horizontal);
    let mut lengths = Lengths::new(CellMetric::DEFAULT, VIEWPORT);
    let mut store = StyleStore::default();

    let mixed = lengths.resolve(
        &width("div { width: calc(16px + 50%) }"),
        horizontal,
        CalcRange::NonNegative,
        &mut store,
    );
    let Some(Value::Calc(handle)) = mixed else {
        panic!("expected a calc, got {}", describe(&mixed));
    };
    // Two cells plus half of a 40-column basis.
    assert_eq!(store.calculations.resolve(handle, 40.0), Some(22.0));

    let clamped = lengths.resolve(
        &width("div { width: clamp(8px, 50%, 40px) }"),
        horizontal,
        CalcRange::NonNegative,
        &mut store,
    );
    let Some(Value::Calc(handle)) = clamped else {
        panic!("expected a calc, got {}", describe(&clamped));
    };
    assert_eq!(
        store.calculations.resolve(handle, 40.0),
        Some(5.0),
        "clamped to the 40px maximum"
    );
    assert_eq!(
        store.calculations.resolve(handle, 2.0),
        Some(1.0),
        "clamped to the 8px minimum"
    );
}

/// A math function our grammar has no notion of. Stylo computes it happily; there is no declaration
/// left to invalidate by mapping time, so the caller falls back to the property's initial value.
#[test]
fn an_unsupported_math_function_is_refused_rather_than_guessed() {
    let value = resolve(
        "div { width: round(up, 17px, 8px) }",
        Axes::same(LengthAxis::Horizontal),
        CalcRange::NonNegative,
    );
    assert!(
        matches!(value, None | Some(Value::Cells(_))),
        "either Stylo simplified it to a length or we refused it; never a wrong calc"
    );
}

// --- the memo --------------------------------------------------------------------------------

/// The memo is what keeps mapping proportional to the number of *distinct* styles. Stylo's sharing
/// cache gives matching elements one `ComputedValues`, and the mapper must not undo that.
#[test]
fn elements_sharing_computed_values_are_mapped_once() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    for _ in 0..64 {
        document.insert_element(Some(body), "div", ElementNs::Html, vec![]);
    }

    let engine = StyloEngine::new(
        CellMetric::DEFAULT,
        VIEWPORT,
        QuirksMode::NoQuirks,
        &["div { color: red }"],
    );
    let arena = StyleArena::new();
    let dom = StyleDom::build(&arena, &document, style::shared_lock::SharedRwLock::new());
    engine.cascade(&dom);

    let (_, stats) = style_tree_measured(&dom, context());
    assert_eq!(stats.elements, 66, "html, body and 64 divs");
    assert!(
        stats.distinct < 8,
        "64 identical divs must not cost 64 mappings; got {} distinct styles",
        stats.distinct
    );
}

// --- the walk --------------------------------------------------------------------------------

/// The walk descends from the document node rather than iterating `StyleDom::elements`, which is
/// unordered — so its coverage is a property worth asserting rather than assuming. A walk that
/// stopped at a text node, or lost a subtree, shows up here.
///
/// `color` is the discriminator because a missing entry reads as `None` while every styled element
/// carries the declared value.
#[test]
fn every_styled_element_reaches_the_tree() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let outer = document.insert_element(Some(body), "div", ElementNs::Html, vec![]);
    // A text node between two elements: the walk has to step over it, not stop at it.
    document.insert_text(Some(outer), "text");
    let inner = document.insert_element(Some(outer), "div", ElementNs::Html, vec![]);
    let deep = document.insert_element(Some(inner), "span", ElementNs::Html, vec![]);
    let sibling = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);

    let engine = StyloEngine::new(
        CellMetric::DEFAULT,
        VIEWPORT,
        QuirksMode::NoQuirks,
        &["html, body, div, span, p { color: #ff0000 }"],
    );
    let arena = StyleArena::new();
    let dom = StyleDom::build(&arena, &document, style::shared_lock::SharedRwLock::new());
    let styled = engine.cascade(&dom);

    let (tree, stats) = style_tree_measured(&dom, context());
    assert_eq!(
        stats.elements, styled,
        "the mapper saw every element Stylo styled"
    );
    for id in [html, body, outer, inner, deep, sibling] {
        assert_eq!(
            tree.get(id).color,
            Some(crate::core::style::Rgba::opaque(
                crate::core::style::Rgb::new(255, 0, 0)
            )),
            "element {id:?} reached the style tree"
        );
    }
}

// --- S4b-2: borders, axes and grid ------------------------------------------------------------

/// The mapping whose *correct* answer looks like a bug.
///
/// `BorderSide::width` is binary — `css/values.rs` computes it as `usize::from(width > 0.0)` and
/// maps `thin`/`medium`/`thick` alike to 1 — because the terminal paints one box-drawing frame for
/// any non-zero width. Stylo computes an undeclared width as `medium`, 3px, and does not zero it
/// for `border-style: none`, so quantising would give `round(3/8) = 0` and erase the frame from
/// every box that declares a style without a width.
#[test]
fn border_widths_are_binary_not_quantised() {
    for css in [
        "div { border-style: solid }",
        "div { border-width: thin; border-style: solid }",
        "div { border-width: 1px; border-style: solid }",
        "div { border-width: 99px; border-style: solid }",
    ] {
        let style = style_of(css);
        assert_eq!(style.border.top.width, 1, "{css}");
        assert!(style.border.is_visible(), "{css}");
    }
    let zero = style_of("div { border-width: 0; border-style: solid }");
    assert_eq!(zero.border.top.width, 0);
    assert!(!zero.border.is_visible());
}

/// Margins and paddings resolve percentages against the inline axis whatever edge they sit on;
/// insets resolve against their own. On a vertical edge that is the difference between a bare
/// percentage and one carrying the column/row ratio, and nothing in the type system says which is
/// which.
#[test]
fn a_vertical_percentage_means_different_things_for_margins_and_insets() {
    use crate::core::style::{CssInset, CssMargin};

    let (tree, div) = mapped("div { margin-top: 50%; top: 50% }");
    let style = tree.get(div);

    let CssInset::Percent(inset) = style.inset.top else {
        panic!(
            "an inset percentage stays a percentage: {:?}",
            style.inset.top
        );
    };
    assert_eq!(inset.basis_points(), 5_000);

    let CssMargin::Calc(margin) = style.margin.top else {
        panic!(
            "a vertical margin percentage must carry the axis ratio: {:?}",
            style.margin.top
        );
    };
    // A 20-column basis is 50% → 10 columns → 5 rows, because a row is two columns tall.
    assert_eq!(tree.resolve_calc(margin, 20.0), Some(5.0));
}

/// A bare `<track-breadth>` is not `minmax(t, t)`: the min side is `auto` for `auto` *and* for
/// `<flex>`, and the breadth otherwise. `fit-content(x)` is `minmax(auto, fit-content(x))`.
#[test]
fn a_bare_track_breadth_is_not_minmax_of_itself() {
    use crate::core::style::{GridLength, GridTemplateComponent, TrackBreadthMax, TrackBreadthMin};

    let single = |css: &str| {
        let (tree, div) = mapped(css);
        let handle = tree
            .get(div)
            .grid
            .template_columns
            .expect("a track list was declared");
        match tree
            .grid()
            .template(handle)
            .expect("the handle resolves")
            .components
            .first()
        {
            Some(GridTemplateComponent::Single(size)) => *size,
            other => panic!("expected one plain track, got {other:?}"),
        }
    };

    let flex = single("div { grid-template-columns: 1fr }");
    assert_eq!(flex.min, TrackBreadthMin::Auto, "`<flex>` mins out at auto");
    assert!(matches!(flex.max, TrackBreadthMax::Fr(_)));

    let length = single("div { grid-template-columns: 16px }");
    assert_eq!(length.min, TrackBreadthMin::Length(GridLength::Cells(2)));
    assert_eq!(length.max, TrackBreadthMax::Length(GridLength::Cells(2)));

    let auto = single("div { grid-template-columns: auto }");
    assert_eq!(auto.min, TrackBreadthMin::Auto);
    assert_eq!(auto.max, TrackBreadthMax::Auto);

    let fit = single("div { grid-template-columns: fit-content(32px) }");
    assert_eq!(fit.min, TrackBreadthMin::Auto);
    assert_eq!(
        fit.max,
        TrackBreadthMax::FitContent(GridLength::Cells(4)),
        "fit-content(x) is minmax(auto, fit-content(x))"
    );

    let minmax = single("div { grid-template-columns: minmax(min-content, 1fr) }");
    assert_eq!(minmax.min, TrackBreadthMin::MinContent);
    assert!(matches!(minmax.max, TrackBreadthMax::Fr(_)));
}

/// CSS's `<fixed-breadth>` is a `<length-percentage>`, which includes `calc()`, so Stylo accepts an
/// auto-repeat track list that Taffy cannot represent. `GridTemplateData::is_valid` is what refuses
/// it, and the mapper falls back to `none` rather than handing layout a list it will discard.
#[test]
fn an_auto_repeat_of_calc_tracks_is_refused() {
    assert!(
        style_of("div { grid-template-columns: repeat(auto-fill, calc(16px + 5%)) }")
            .grid
            .template_columns
            .is_none(),
        "calc() is not a fixed component, so the whole list is refused"
    );
    assert!(
        style_of("div { grid-template-columns: repeat(auto-fill, 16px) }")
            .grid
            .template_columns
            .is_some(),
        "a fixed track keeps the same list valid"
    );
}

/// `grid-auto-flow` is bitflags in Stylo — `ROW | COLUMN | DENSE` — not the four-variant enum we
/// hold, so the axis and the packing are read separately.
#[test]
fn grid_auto_flow_reads_both_bits() {
    use crate::core::style::GridAutoFlow;

    for (declared, expected) in [
        ("row", GridAutoFlow::Row),
        ("column", GridAutoFlow::Column),
        ("row dense", GridAutoFlow::RowDense),
        ("column dense", GridAutoFlow::ColumnDense),
        ("dense", GridAutoFlow::RowDense),
    ] {
        let css = format!("div {{ grid-auto-flow: {declared} }}");
        assert_eq!(style_of(&css).grid.auto_flow, expected, "{declared}");
    }
}

/// A named line with no explicit index is the *first* line of that name, which our model stores as
/// index 1 rather than Stylo's 0.
#[test]
fn a_bare_named_grid_line_is_index_one() {
    use crate::core::style::GridPlacement;

    let (tree, div) = mapped("div { grid-row-start: foo; grid-column-start: span bar }");
    let style = tree.get(div);
    match style.grid.row.start {
        GridPlacement::NamedLine(name, index) => {
            assert_eq!(tree.grid().ident(name), Some("foo"));
            assert_eq!(index, 1);
        }
        other => panic!("expected a named line, got {other:?}"),
    }
    match style.grid.column.start {
        GridPlacement::NamedSpan(name, count) => {
            assert_eq!(tree.grid().ident(name), Some("bar"));
            assert_eq!(count, 1);
        }
        other => panic!("expected a named span, got {other:?}"),
    }
}

/// Stylo's `NamedArea` already numbers rows and columns as 1-based, end-exclusive grid lines, which
/// is exactly what `GridAreasData` wants — so this pins "no translation" rather than assuming it.
#[test]
fn grid_area_coordinates_need_no_translation() {
    let (tree, div) = mapped(r#"div { grid-template-areas: "a a b" "c c b" }"#);
    let handle = tree
        .get(div)
        .grid
        .template_areas
        .expect("the areas were declared");
    let data = tree.grid().areas(handle).expect("the handle resolves");
    assert_eq!((data.row_count, data.column_count), (2, 3));

    let named = |want: &str| {
        *data
            .areas
            .iter()
            .find(|area| tree.grid().ident(area.name) == Some(want))
            .unwrap_or_else(|| panic!("area {want} is present"))
    };
    let a = named("a");
    assert_eq!((a.row_start, a.row_end), (1, 2), "one row tall, lines 1..2");
    assert_eq!(
        (a.column_start, a.column_end),
        (1, 3),
        "two columns wide, lines 1..3"
    );
    let b = named("b");
    assert_eq!((b.row_start, b.row_end), (1, 3), "two rows tall");
    assert_eq!((b.column_start, b.column_end), (3, 4));
}
