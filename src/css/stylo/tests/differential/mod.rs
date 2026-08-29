//! The M7 differential oracle: both cascades, one process, one assertion.
//!
//! **Every property a test reads must be declared by that test's own CSS.** The Stylo user-agent
//! sheet is still the S3 stub and genuinely disagrees with `css/ua.rs` — the stub gives `<p>` a
//! `1em` top *and* bottom margin where the real sheet gives one cell at the bottom only, and it has
//! no heading `font-size` scaling at all. An author declaration outranks both user-agent origins,
//! so declaring the property under test makes that difference irrelevant rather than papering over
//! it. A test that reads an *undeclared* property is asserting user-agent parity, which is S4c's
//! job, not this file's.
//!
//! **`controls.rs` is the one exception, and it is not a loophole.** `ComputedStyle::reverse` has
//! no CSS property in *either* engine: both derive it from the element, through the same
//! `core::form::control_kind`. The rule above exists because the two user-agent sheets disagree,
//! and a value no sheet can express cannot inherit that disagreement.

mod controls;
mod inline;
mod properties;

use std::fmt::Debug;

use crate::core::dom::{Attr, Document, NodeId};
use crate::core::style::{ComputedStyle, RenderContext, StyleTree};
use crate::css::stylo::map;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};

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
    let dom = mirror(&engine, &arena, document);
    engine.cascade(&dom);
    map::style_tree(&dom, RenderContext::terminal(VIEWPORT))
}

fn div(count: usize) -> Vec<(&'static str, Vec<Attr>)> {
    vec![("div", Vec::new()); count]
}
