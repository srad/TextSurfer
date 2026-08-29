//! The M7 differential oracle: both cascades, one process, one assertion.
//!
//! **Every property a test reads must be declared by that test's own CSS.** The Stylo user-agent
//! sheet is still the S3 stub and genuinely disagrees with `css/ua.rs` — the stub gives `<p>` a
//! `1em` top *and* bottom margin where the real sheet gives one cell at the bottom only, and it has
//! no heading `font-size` scaling at all. An author declaration outranks both user-agent origins,
//! so declaring the property under test makes that difference irrelevant rather than papering over
//! it. A test that reads an *undeclared* property is asserting user-agent parity, which is S4c's
//! job, not this file's.

use std::fmt::Debug;

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::style::VerticalAlign;
use crate::core::style::{ComputedStyle, RenderContext, StyleTree};
use crate::css::stylo::map;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};

use super::super::dom::StyleArena;
use super::super::engine::StyloEngine;
use super::resolved;
use super::{VIEWPORT, document, mirror};

/// The same document and stylesheet, cascaded by both engines.
struct Both {
    custom: StyleTree,
    stylo: StyleTree,
    ids: Vec<NodeId>,
}

impl Both {
    /// Assert the two engines agree on one projection of one element's style.
    ///
    /// A projection rather than the whole struct: see the module comment.
    fn agree<T: Debug + PartialEq>(
        &self,
        index: usize,
        property: &str,
        project: impl Fn(ComputedStyle) -> T,
    ) {
        let id = self.ids[index];
        let custom = project(self.custom.get(id));
        let stylo = project(self.stylo.get(id));
        assert_eq!(
            custom, stylo,
            "{property} on element {index}: custom cascade said {custom:?}, Stylo said {stylo:?}"
        );
    }

    /// [`Self::agree`] for values that `ComputedStyle` holds by handle.
    ///
    /// `CssCalc` and the grid handles index the `StyleStore` of the tree that produced them, so the
    /// projection is handed its own tree and resolves through it. Comparing the raw handles would
    /// compare interning order, which passes by luck whenever a test declares exactly one value.
    fn agree_resolved<T: Debug + PartialEq>(
        &self,
        index: usize,
        property: &str,
        project: impl Fn(&StyleTree, ComputedStyle) -> T,
    ) {
        let id = self.ids[index];
        let custom = project(&self.custom, self.custom.get(id));
        let stylo = project(&self.stylo, self.stylo.get(id));
        assert_eq!(
            custom, stylo,
            "{property} on element {index}: custom cascade said {custom:?}, Stylo said {stylo:?}"
        );
    }

    fn stylo(&self, index: usize) -> ComputedStyle {
        self.stylo.get(self.ids[index])
    }
}

/// `ids` are the elements of `body`, in the order given — index 0 is the first entry, not `<html>`.
fn both(css: &str, body: &[(&str, Vec<Attr>)]) -> Both {
    let (document, ids) = document(body);
    Both {
        custom: custom_cascade(css, &document),
        stylo: stylo_cascade(css, &document),
        // `document` returns `[html, body, ..body]`; tests name their own elements.
        ids: ids[2..].to_vec(),
    }
}

fn custom_cascade(css: &str, document: &Document) -> StyleTree {
    let sheet = CssparserParser.parse(css);
    BasicCascade.apply(&[sheet], document, MediaContext::screen())
}

fn stylo_cascade(css: &str, document: &Document) -> StyleTree {
    let engine = StyloEngine::new(
        RenderContext::terminal(VIEWPORT).metrics.cell,
        VIEWPORT,
        style::context::QuirksMode::NoQuirks,
        &[css],
    );
    let arena = StyleArena::new();
    let dom = mirror(&arena, document);
    engine.cascade(&dom);
    map::style_tree(&dom, RenderContext::terminal(VIEWPORT))
}

fn div(count: usize) -> Vec<(&'static str, Vec<Attr>)> {
    vec![("div", Vec::new()); count]
}

#[test]
fn white_space_modes_agree() {
    for (declared, _) in [
        ("normal", ()),
        ("nowrap", ()),
        ("pre", ()),
        ("pre-wrap", ()),
        ("pre-line", ()),
        ("break-spaces", ()),
    ] {
        let css = format!("div {{ white-space: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "white-space", |style| style.white_space);
    }
}

#[test]
fn text_align_keywords_agree() {
    for declared in ["start", "left", "right", "center", "justify"] {
        let css = format!("div {{ text-align: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "text-align", |style| style.text_align);
    }
}

#[test]
fn colours_agree_including_alpha() {
    for declared in [
        "red",
        "#00ff00",
        "rgb(1 2 4)",
        "rgba(10, 20, 30, 0.5)",
        "hsl(120 100% 50%)",
    ] {
        let css = format!("div {{ color: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "color", |style| style.color);
    }
}

#[test]
fn background_colours_agree() {
    for declared in ["blue", "#123456", "transparent"] {
        let css = format!("div {{ background-color: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "background-color", |style| style.background);
    }
}

/// The sentinel's whole job: an element with no author `color` must reach layout as "the theme
/// decides", not as Stylo's initial black.
#[test]
fn an_undeclared_colour_is_none_on_both_sides() {
    let both = both("div { text-align: left }", &div(1));
    assert_eq!(both.stylo(0).color, None);
    both.agree(0, "color", |style| style.color);
}

#[test]
fn font_weight_maps_to_the_bold_bit() {
    for declared in ["normal", "bold", "100", "500", "501", "700"] {
        let css = format!("div {{ font-weight: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "font-weight", |style| style.bold);
    }
}

#[test]
fn text_decorations_agree() {
    for declared in [
        "none",
        "underline",
        "line-through",
        "underline line-through",
    ] {
        let css = format!("div {{ text-decoration-line: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "text-decoration-line", |style| {
            (style.underline, style.strike)
        });
    }
}

#[test]
fn font_size_and_its_presentation_agree() {
    for declared in ["8px", "16px", "2em", "150%", "24px", "32px"] {
        let css = format!("div {{ font-size: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "font-size", |style| style.font_size.px());
        both.agree(0, "text-presentation", |style| style.text_presentation);
    }
}

#[test]
fn cursor_keywords_agree() {
    for declared in ["auto", "default", "pointer", "text", "none", "nwse-resize"] {
        let css = format!("div {{ cursor: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "cursor", |style| style.cursor);
    }
}

#[test]
fn visibility_and_zero_opacity_agree() {
    for css in [
        "div { visibility: visible }",
        "div { visibility: hidden }",
        "div { visibility: collapse }",
        "div { opacity: 0 }",
        "div { opacity: 1 }",
    ] {
        let both = both(css, &div(1));
        both.agree(0, "visibility", |style| style.visibility);
    }
}

#[test]
fn list_style_keywords_agree() {
    for declared in [
        "none",
        "disc",
        "circle",
        "square",
        "decimal",
        "decimal-leading-zero",
        "lower-alpha",
        "upper-alpha",
        "lower-roman",
        "upper-roman",
    ] {
        let css = format!("div {{ list-style-type: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "list-style-type", |style| style.list_style_type);
    }
    for declared in ["inside", "outside"] {
        let css = format!("div {{ list-style-position: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "list-style-position", |style| style.list_style_position);
    }
}

#[test]
fn table_keywords_agree() {
    for css in [
        "div { table-layout: fixed }",
        "div { table-layout: auto }",
        "div { border-collapse: collapse }",
        "div { border-collapse: separate }",
        "div { caption-side: bottom }",
        "div { caption-side: top }",
    ] {
        let both = both(css, &div(1));
        both.agree(0, "table keyword", |style| {
            (
                style.table_layout,
                style.border_collapse,
                style.caption_side,
            )
        });
    }
}

#[test]
fn border_spacing_quantises_to_cells_on_both_sides() {
    for declared in ["0", "8px", "8px 16px", "1em"] {
        let css = format!("div {{ border-spacing: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "border-spacing", |style| style.border_spacing);
    }
}

/// `text-decoration-line` is a *reset* property in Stylo — `properties/data.py` puts it in the
/// non-inherited `Text` struct — while `ComputedStyle::underline` and `strike` inherit, which is
/// what makes every descendant of a link underlined. Without `map::policy`'s propagation a
/// per-element mapper would strip the decoration from the inner element and nothing else would
/// notice until a page rendered wrong.
///
/// An author rule, not the user-agent link rule: the Stylo user-agent sheet has no link styling
/// until S4c, and the oracle's invariant is that the property under test is declared here.
#[test]
fn decorations_propagate_to_descendants() {
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
    let outside = document.insert_element(Some(body), "span", ElementNs::Html, vec![]);

    let css = "a { text-decoration-line: underline } b { text-decoration-line: line-through }";
    let both = Both {
        custom: custom_cascade(css, &document),
        stylo: stylo_cascade(css, &document),
        ids: vec![anchor, inner, outside],
    };

    assert!(both.stylo(0).underline, "the anchor itself");
    assert!(
        both.stylo(1).underline,
        "the inner span inherits the anchor's underline"
    );
    assert!(
        !both.stylo(2).underline,
        "a span outside the anchor does not"
    );
    for index in 0..3 {
        both.agree(index, "underline", |style| style.underline);
    }
}

/// The M7 divergence ledger, made executable.
///
/// Stylo 0.20 has no CSS 2.1 `vertical-align`: the shorthand expands to `alignment-baseline`, whose
/// servo build accepts `baseline | middle | text-top | text-bottom` and *not* `top` or `bottom`. So
/// `middle` must agree and `top` must not. Asserting the disagreement is also what proves this
/// oracle has teeth — a harness that silently compared one engine with itself would pass every
/// other test in this file and fail this one.
#[test]
fn vertical_align_agrees_on_middle_and_diverges_on_top() {
    let agreeing = both("div { vertical-align: middle }", &div(1));
    agreeing.agree(0, "vertical-align", |style| style.vertical_align);
    assert_eq!(agreeing.stylo(0).vertical_align, VerticalAlign::Middle);

    let diverging = both("div { vertical-align: top }", &div(1));
    assert_eq!(
        diverging.custom.get(diverging.ids[0]).vertical_align,
        VerticalAlign::Top,
        "the custom cascade honours the CSS 2.1 keyword"
    );
    assert_eq!(
        diverging.stylo(0).vertical_align,
        VerticalAlign::Baseline,
        "Stylo cannot parse it, so the declaration never applies. If this ever becomes `Top`, \
         Stylo grew the keyword and the divergence ledger entry can go."
    );
}

/// Inherited properties are the ones a per-element mapper is most likely to get wrong, so they get
/// a parent/child pair rather than a single element.
#[test]
fn inherited_properties_reach_a_child() {
    let both = both(
        "div { color: #ff8800; white-space: pre; cursor: help; text-align: center; \
         font-size: 24px; list-style-type: square; visibility: hidden }",
        &[("div", Vec::new())],
    );
    both.agree(0, "inherited set", |style| {
        (
            style.color,
            style.white_space,
            style.cursor,
            style.text_align,
            style.font_size.px(),
            style.list_style_type,
            style.visibility,
        )
    });
}

// --- S4b-2: the box families -------------------------------------------------------------------

#[test]
fn display_keywords_agree() {
    for declared in [
        "none",
        "contents",
        "block",
        "inline",
        "inline-block",
        "flow-root",
        "list-item",
        "table",
        "inline-table",
        "table-row-group",
        "table-header-group",
        "table-footer-group",
        "table-row",
        "table-cell",
        "table-column",
        "table-column-group",
        "table-caption",
        "flex",
        "inline-flex",
        "grid",
        "inline-grid",
    ] {
        let css = format!("div {{ display: {declared} }}");
        let both = both(&css, &div(1));
        both.agree(0, "display", |style| style.display);
    }
}

#[test]
fn sizes_agree_including_percentages_and_calc() {
    for declared in ["auto", "0", "24px", "50%", "calc(16px + 25%)", "3em"] {
        for property in ["width", "height", "min-width", "min-height"] {
            let css = format!("div {{ {property}: {declared} }}");
            let both = both(&css, &div(1));
            both.agree_resolved(0, property, |tree, style| {
                [
                    resolved::size(tree, style.width),
                    resolved::size(tree, style.height),
                    resolved::size(tree, style.min_width),
                    resolved::size(tree, style.min_height),
                ]
            });
        }
    }
    for declared in ["none", "24px", "50%", "calc(16px + 25%)"] {
        for property in ["max-width", "max-height"] {
            let css = format!("div {{ {property}: {declared} }}");
            let both = both(&css, &div(1));
            both.agree_resolved(0, property, |tree, style| {
                [
                    resolved::max_size(tree, style.max_width),
                    resolved::max_size(tree, style.max_height),
                ]
            });
        }
    }
}

/// Two calc-bearing declarations in one rule: the interning order differs between the engines, so
/// this is the case that a handle comparison gets wrong and a resolved comparison gets right.
#[test]
fn several_calcs_in_one_rule_agree_regardless_of_interning_order() {
    let both = both(
        "div { width: calc(16px + 25%); height: calc(32px - 10%); \
         margin-left: calc(8px + 5%); padding-top: calc(4px + 2%) }",
        &div(1),
    );
    both.agree_resolved(0, "mixed calc", |tree, style| {
        [
            resolved::size(tree, style.width),
            resolved::size(tree, style.height),
            resolved::margin(tree, style.margin.left),
            resolved::padding(tree, style.padding.top),
        ]
    });
}

#[test]
fn box_keywords_agree() {
    for css in [
        "div { box-sizing: border-box }",
        "div { box-sizing: content-box }",
        "div { position: static }",
        "div { position: relative }",
        "div { position: absolute }",
        "div { position: fixed }",
        "div { position: sticky }",
        "div { float: left }",
        "div { float: right }",
        "div { float: none }",
        "div { clear: left }",
        "div { clear: right }",
        "div { clear: both }",
        "div { order: 3 }",
        "div { order: -2 }",
    ] {
        let both = both(css, &div(1));
        both.agree(0, "box keyword", |style| {
            (
                style.box_sizing,
                style.position,
                style.float,
                style.clear,
                style.order,
            )
        });
    }
}

/// Includes the cross-axis fixup: a specified `visible` computes to `auto` when the other axis is
/// scrollable, which both engines must apply.
#[test]
fn overflow_agrees_including_the_cross_axis_fixup() {
    for css in [
        "div { overflow: visible }",
        "div { overflow: hidden }",
        "div { overflow: clip }",
        "div { overflow: scroll }",
        "div { overflow: auto }",
        "div { overflow-x: hidden }",
        "div { overflow-y: scroll }",
        "div { overflow-x: clip; overflow-y: visible }",
        "div { overflow-x: visible; overflow-y: auto }",
    ] {
        let both = both(css, &div(1));
        both.agree(0, "overflow", |style| style.overflow);
    }
}

/// Insets resolve percentages against their own axis; margins and paddings against the inline axis.
/// A vertical percentage is where those two rules visibly part company.
#[test]
fn edges_agree_on_both_axes() {
    for declared in ["0", "16px", "auto", "25%", "calc(8px + 10%)", "-8px"] {
        for property in ["top", "bottom", "left", "right", "margin", "margin-top"] {
            let css = format!("div {{ {property}: {declared} }}");
            let both = both(&css, &div(1));
            both.agree_resolved(0, property, |tree, style| {
                [
                    resolved::inset(tree, style.inset.top),
                    resolved::inset(tree, style.inset.right),
                    resolved::inset(tree, style.inset.bottom),
                    resolved::inset(tree, style.inset.left),
                    resolved::margin(tree, style.margin.top),
                    resolved::margin(tree, style.margin.right),
                    resolved::margin(tree, style.margin.bottom),
                    resolved::margin(tree, style.margin.left),
                ]
            });
        }
    }
    for declared in ["0", "16px", "25%", "calc(8px + 10%)"] {
        for property in ["padding", "padding-top", "padding-left"] {
            let css = format!("div {{ {property}: {declared} }}");
            let both = both(&css, &div(1));
            both.agree_resolved(0, property, |tree, style| {
                [
                    resolved::padding(tree, style.padding.top),
                    resolved::padding(tree, style.padding.right),
                    resolved::padding(tree, style.padding.bottom),
                    resolved::padding(tree, style.padding.left),
                ]
            });
        }
    }
}

/// The mapping whose correct answer looks like a bug: `BorderSide::width` is binary, so Stylo's 3px
/// `medium` must come out as `1`, not `round(3/8) = 0`.
#[test]
fn borders_agree_and_stay_binary() {
    for css in [
        "div { border-style: solid }",
        "div { border: 1px solid red }",
        "div { border-width: thin; border-style: solid }",
        "div { border-width: thick; border-style: dashed }",
        "div { border-width: 0; border-style: solid }",
        "div { border-top: 2px dotted #00ff00 }",
        "div { border-style: solid; border-color: transparent }",
        "div { border-left-style: double; border-left-width: 4px }",
    ] {
        let both = both(css, &div(1));
        both.agree(0, "border", |style| style.border);
    }
    let solid = both("div { border-style: solid }", &div(1));
    assert_eq!(
        solid.stylo(0).border.top.width,
        1,
        "an undeclared width is `medium`; quantising it would erase the frame"
    );
    assert!(solid.stylo(0).border.is_visible());
}

#[test]
fn flex_properties_agree() {
    for css in [
        "div { flex-direction: row }",
        "div { flex-direction: row-reverse }",
        "div { flex-direction: column }",
        "div { flex-direction: column-reverse }",
        "div { flex-wrap: nowrap }",
        "div { flex-wrap: wrap }",
        "div { flex-wrap: wrap-reverse }",
        "div { flex-grow: 2 }",
        "div { flex-shrink: 0 }",
        "div { flex-basis: auto }",
        "div { flex-basis: content }",
        "div { flex-basis: 32px }",
        "div { flex-basis: 50% }",
        "div { flex: 1 1 0 }",
        "div { flex: none }",
    ] {
        let both = both(css, &div(1));
        both.agree_resolved(0, "flex", |tree, style| {
            (
                style.flex.direction,
                style.flex.wrap,
                style.flex.grow,
                style.flex.shrink,
                resolved::flex_basis(tree, style.flex.basis),
            )
        });
    }
}

#[test]
fn alignment_and_gaps_agree() {
    for css in [
        "div { justify-content: center }",
        "div { justify-content: space-between }",
        "div { justify-content: space-around }",
        "div { justify-content: space-evenly }",
        "div { justify-content: flex-start }",
        "div { justify-content: left }",
        "div { justify-content: safe center }",
        "div { align-content: stretch }",
        "div { align-content: end }",
        "div { align-items: center }",
        "div { align-items: baseline }",
        "div { align-items: stretch }",
        "div { align-items: self-start }",
        "div { justify-items: left }",
        "div { align-self: flex-end }",
        "div { justify-self: unsafe center }",
        "div { row-gap: 16px }",
        "div { column-gap: 8px }",
        "div { gap: 16px 8px }",
        "div { row-gap: 25% }",
        "div { gap: normal }",
    ] {
        let both = both(css, &div(1));
        both.agree_resolved(0, "alignment", |tree, style| {
            (
                style.alignment.justify_content,
                style.alignment.align_content,
                style.alignment.justify_items,
                style.alignment.align_items,
                style.alignment.justify_self,
                style.alignment.align_self,
                resolved::gap(tree, style.alignment.row_gap),
                resolved::gap(tree, style.alignment.column_gap),
            )
        });
    }
}

#[test]
fn grid_templates_and_placement_agree() {
    for css in [
        "div { grid-template-columns: 1fr 2fr }",
        "div { grid-template-columns: 16px 50% auto }",
        "div { grid-template-columns: minmax(16px, 1fr) }",
        "div { grid-template-columns: fit-content(32px) }",
        "div { grid-template-columns: repeat(3, 16px) }",
        "div { grid-template-columns: repeat(auto-fill, 16px) }",
        "div { grid-template-columns: repeat(auto-fit, 24px) }",
        "div { grid-template-columns: [a] 16px [b] 32px [c] }",
        "div { grid-template-rows: min-content max-content }",
        "div { grid-auto-columns: 16px }",
        "div { grid-auto-rows: minmax(8px, auto) }",
        "div { grid-auto-flow: row }",
        "div { grid-auto-flow: column }",
        "div { grid-auto-flow: row dense }",
        "div { grid-auto-flow: column dense }",
        "div { grid-row-start: 2 }",
        "div { grid-row-end: -1 }",
        "div { grid-column-start: span 3 }",
        "div { grid-column-end: auto }",
    ] {
        let both = both(css, &div(1));
        both.agree_resolved(0, "grid", |tree, style| resolved::grid(tree, style.grid));
    }
}

/// The shorthands are Stylo's job: it expands `grid-area` into the four longhands and applies the
/// "a bare name implies the same name on the end line" rule itself, so the mapper never sees them.
/// This is what proves that claim rather than assuming it.
#[test]
fn grid_shorthands_expand_the_same_way() {
    for css in [
        "div { grid-row: 2 / 4 }",
        "div { grid-column: span 2 / 5 }",
        "div { grid-area: 1 / 2 / 3 / 4 }",
        "div { grid-template-areas: \"a a\" \"b c\" }",
        "div { grid-template: \"a a\" 16px \"b c\" 32px / 1fr 2fr }",
    ] {
        let both = both(css, &div(1));
        both.agree_resolved(0, "grid shorthand", |tree, style| {
            resolved::grid(tree, style.grid)
        });
    }
}

/// The M7 divergence ledger, made executable — the sizing half.
///
/// The intrinsic keywords have no `CssSize` variant. Stylo computes them and the mapper folds them
/// to `Auto`; the custom cascade dropped the declaration instead, leaving `width: 32px` standing.
#[test]
fn intrinsic_size_keywords_diverge() {
    let both = both("div { width: 32px; width: min-content }", &div(1));
    assert_eq!(
        both.custom.get(both.ids[0]).width,
        crate::core::style::CssSize::Cells(4),
        "the custom cascade drops the keyword and keeps the previous declaration"
    );
    assert_eq!(
        both.stylo(0).width,
        crate::core::style::CssSize::Auto,
        "Stylo computes the keyword and the mapper has no variant for it"
    );
}
