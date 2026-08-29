//! The M7 differential oracle: both cascades, one process, one assertion.
//!
//! Property tests declare their own author CSS and compare named projections. UA and
//! presentational-hint tests deliberately read undeclared properties and compare complete styles.

mod controls;
mod inline;
mod properties;
mod ua;

use std::fmt::Debug;

use crate::core::dom::{Attr, Document, NodeId};
use crate::core::style::{ComputedStyle, Palette, RenderContext, StyleTree};
use crate::css::stylo::map;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use stylo_dom::ElementState;

use super::super::dom::StyleArena;
use super::super::engine::StyloEngine;
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

    fn agree_all(&self, index: usize) {
        self.agree(index, "ComputedStyle", |style| style);
    }
}

/// `ids` are the elements of `body`, in the order given — index 0 is the first entry, not `<html>`.
fn both(css: &str, body: &[(&str, Vec<Attr>)]) -> Both {
    both_with_palette(css, body, Palette::default())
}

fn both_with_palette(css: &str, body: &[(&str, Vec<Attr>)], palette: Palette) -> Both {
    let (document, ids) = document(body);
    Both {
        custom: custom_cascade_with_palette(css, &document, palette),
        stylo: stylo_cascade_with_palette(css, &document, palette),
        // `document` returns `[html, body, ..body]`; tests name their own elements.
        ids: ids[2..].to_vec(),
    }
}

fn custom_cascade(css: &str, document: &Document) -> StyleTree {
    custom_cascade_with_palette(css, document, Palette::default())
}

fn custom_cascade_with_palette(css: &str, document: &Document, palette: Palette) -> StyleTree {
    let sheet = CssparserParser.parse(css);
    BasicCascade.apply(
        &[sheet],
        document,
        MediaContext::screen().with_palette(palette),
    )
}

fn stylo_cascade(css: &str, document: &Document) -> StyleTree {
    stylo_cascade_with_palette(css, document, Palette::default())
}

fn stylo_cascade_with_palette(css: &str, document: &Document, palette: Palette) -> StyleTree {
    let engine = StyloEngine::new(
        RenderContext::terminal(VIEWPORT).metrics.cell,
        VIEWPORT,
        style::context::QuirksMode::NoQuirks,
        palette,
        &[css],
    );
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, document);
    engine.cascade(&dom);
    map::style_tree(&dom, RenderContext::terminal(VIEWPORT))
}

fn stylo_cascade_with_palette_and_state(
    css: &str,
    document: &Document,
    palette: Palette,
    id: NodeId,
    state: ElementState,
) -> StyleTree {
    let engine = StyloEngine::new(
        RenderContext::terminal(VIEWPORT).metrics.cell,
        VIEWPORT,
        style::context::QuirksMode::NoQuirks,
        palette,
        &[css],
    );
    let arena = StyleArena::new();
    let dom = mirror(&engine, &arena, document);
    dom.element(id)
        .expect("a mirrored element")
        .set_state(state);
    engine.cascade(&dom);
    map::style_tree(&dom, RenderContext::terminal(VIEWPORT))
}

fn div(count: usize) -> Vec<(&'static str, Vec<Attr>)> {
    vec![("div", Vec::new()); count]
}
