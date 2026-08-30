#![allow(unsafe_code)]

mod build;
mod traits;

pub(super) use build::static_state;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::collections::HashMap;

use selectors::matching::ElementSelectorFlags;
use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::data::ElementDataWrapper;
use style::properties::{ComputedValues, LonghandId, PropertyDeclarationBlock};
use style::rule_tree::CascadeOrigin;
use style::shared_lock::{Locked, SharedRwLock};
use style::values::{AtomIdent, AtomString};
use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace};

use crate::core::dom::NodeId;
use crate::core::form::ControlKind;
use crate::core::style::LegacyAlign;

/// The backing store for a [`StyleDom`].
///
/// Nodes link to each other by reference rather than by index because Stylo's style sharing cache
/// requires an element handle to be exactly pointer-sized: `FakeCandidate` declares `_element:
/// usize` and `StyleSharingCache::new` asserts the two layouts match. An index-plus-arena handle is
/// two words and trips that assertion. A `typed_arena::Arena` gives every node the same lifetime,
/// which is what lets the links be safe shared references instead of raw pointers.
pub(crate) type StyleArena<'a> = typed_arena::Arena<StyleNode<'a>>;

/// An owned mirror of the parts of `core::dom::Document` that Stylo needs to see.
///
/// Stylo requires interior-mutable per-element style data, state bits, selector flags and a stable
/// identity. None of that can live on `core::dom::Node` without pulling `style::` into `core`, so
/// the style engine gets its own tree. Nodes map back through [`StyleNode::dom_id`].
pub(crate) struct StyleDom<'a> {
    document: &'a StyleNode<'a>,
    by_dom_id: HashMap<NodeId, &'a StyleNode<'a>>,
    len: usize,
}

pub(crate) struct StyleNode<'a> {
    /// The document node this node hangs under, so `owner_doc` is a field read rather than a walk.
    /// `None` on the document node itself.
    document: Cell<Option<&'a StyleNode<'a>>>,
    dom_id: Option<NodeId>,
    parent: Cell<Option<&'a StyleNode<'a>>>,
    first_child: Cell<Option<&'a StyleNode<'a>>>,
    last_child: Cell<Option<&'a StyleNode<'a>>>,
    prev_sibling: Cell<Option<&'a StyleNode<'a>>>,
    next_sibling: Cell<Option<&'a StyleNode<'a>>>,
    kind: NodeKind,
}

pub(crate) enum NodeKind {
    Document(DocumentNode),
    Text,
    Element(ElementNode),
}

/// Document-wide state. It lives on the document node so `TDocument`, whose only handle is a node
/// reference, can reach it without a second pointer on every node.
pub(crate) struct DocumentNode {
    quirks_mode: QuirksMode,
    lock: SharedRwLock,
}

pub(crate) struct ElementNode {
    local_name: LocalName,
    namespace: Namespace,
    id: Option<AtomIdent>,
    classes: Vec<AtomIdent>,
    attrs: Vec<MirrorAttr>,
    /// What kind of form control this element is, resolved once at build time through
    /// `core::form::control_kind` — the same answer `css::ua` gives the custom cascade.
    ///
    /// It has to be carried rather than recomputed because it is not a CSS question: the mapper
    /// sees `ComputedValues`, which say nothing about the element's tag or its `type` attribute.
    control: Option<ControlKind>,
    state: Cell<ElementState>,
    selector_flags: Cell<ElementSelectorFlags>,
    data: ElementDataWrapper,
    /// Stylo allocates `ElementData` lazily: `has_data`/`borrow_data` must report nothing until
    /// `ensure_data` has run. `with_default_parent_styles` reads `borrow_data().styles.primary()`
    /// — an `unwrap` — for any parent that reports data, so claiming data unconditionally panics
    /// the moment a child is resolved before its parent.
    allocated: Cell<bool>,
    style_attribute: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
    presentational_hints: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
    legacy_align: Option<LegacyAlign>,
    children_to_process: Cell<isize>,
    dirty_descendants: Cell<bool>,
    has_snapshot: Cell<bool>,
    handled_snapshot: Cell<bool>,
}

pub(crate) struct MirrorAttr {
    pub(crate) name: LocalName,
    pub(crate) namespace: Namespace,
    pub(crate) value: AtomString,
}

impl<'a> StyleDom<'a> {
    pub(crate) fn document(&self) -> StyloDocument<'a> {
        StyloDocument(self.document)
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// The document element — the first element child of the document node.
    pub(crate) fn root_element(&self) -> Option<StyloElement<'a>> {
        let mut child = self.document.first_child.get();
        while let Some(node) = child {
            if matches!(node.kind, NodeKind::Element(_)) {
                return Some(StyloElement(node));
            }
            child = node.next_sibling.get();
        }
        None
    }

    /// How many elements carry a resolved primary style.
    ///
    /// Reported beside a cascade timing so a fast traversal that skipped most of the tree cannot be
    /// mistaken for a fast traversal that did the work.
    pub(crate) fn styled_element_count(&self) -> usize {
        self.by_dom_id
            .values()
            .filter_map(|node| StyloElement::new(node))
            .filter(|element| element.primary_style().is_some())
            .count()
    }

    /// Every mirrored element, in no particular order.
    pub(crate) fn elements(&self) -> impl Iterator<Item = StyloElement<'a>> + '_ {
        self.by_dom_id
            .values()
            .filter_map(|node| StyloElement::new(node))
    }

    /// The mirrored element for a document node, if it was mirrored and is an element.
    pub(crate) fn element(&self, id: NodeId) -> Option<StyloElement<'a>> {
        self.by_dom_id
            .get(&id)
            .copied()
            .filter(|node| matches!(node.kind, NodeKind::Element(_)))
            .map(StyloElement)
    }
}

impl<'a> StyleNode<'a> {
    fn element(&self) -> Option<&ElementNode> {
        match &self.kind {
            NodeKind::Element(element) => Some(element),
            _ => None,
        }
    }

    fn next_sibling_node(&self) -> Option<&'a StyleNode<'a>> {
        self.next_sibling.get()
    }

    fn prev_sibling_node(&self) -> Option<&'a StyleNode<'a>> {
        self.prev_sibling.get()
    }

    fn owner_document(&self) -> Option<&'a StyleNode<'a>> {
        self.document.get()
    }
}

/// A `Copy` handle. Exactly one pointer wide, which is what Stylo's sharing cache demands.
#[derive(Clone, Copy)]
pub(crate) struct StyloNode<'a>(pub(crate) &'a StyleNode<'a>);

#[derive(Clone, Copy)]
pub(crate) struct StyloElement<'a>(pub(crate) &'a StyleNode<'a>);

#[derive(Clone, Copy)]
pub(crate) struct StyloDocument<'a>(pub(crate) &'a StyleNode<'a>);

impl<'a> StyloElement<'a> {
    pub(crate) fn new(node: &'a StyleNode<'a>) -> Option<Self> {
        matches!(node.kind, NodeKind::Element(_)).then_some(Self(node))
    }

    fn element(&self) -> &'a ElementNode {
        self.0
            .element()
            .expect("a StyloElement is only built over an element node")
    }

    pub(crate) fn dom_id(&self) -> Option<NodeId> {
        self.0.dom_id
    }

    pub(crate) fn control_kind(&self) -> Option<ControlKind> {
        self.element().control
    }

    pub(crate) fn legacy_align(&self) -> Option<LegacyAlign> {
        self.element().legacy_align
    }

    pub(crate) fn has_author_text_align(&self, values: &ComputedValues) -> bool {
        let Some(rules) = values.rules.as_ref() else {
            return false;
        };
        let document = match &self
            .0
            .owner_document()
            .expect("an element has an owner document")
            .kind
        {
            NodeKind::Document(document) => document,
            _ => unreachable!("an element's owner document is a document node"),
        };
        let guard = document.lock.read();
        rules.self_and_ancestors().any(|node| {
            node.cascade_level().origin() == CascadeOrigin::Author
                && node.style_source().is_some_and(|source| {
                    source
                        .read(&guard)
                        .declarations()
                        .iter()
                        .any(|declaration| {
                            declaration.id().as_longhand() == Some(LonghandId::TextAlign)
                        })
                })
        })
    }

    pub(crate) fn state(&self) -> ElementState {
        self.element().state.get()
    }

    pub(crate) fn set_state(&self, state: ElementState) {
        self.element().state.set(state);
    }

    /// Tell Stylo a snapshot of this element's previous state is in the snapshot map.
    pub(crate) fn note_snapshot(&self) {
        let element = self.element();
        element.has_snapshot.set(true);
        element.handled_snapshot.set(false);
    }

    /// Forget any snapshot this element carries.
    ///
    /// Stylo never clears `has_snapshot` — it only ever sets `handled_snapshot` — so an embedder
    /// that reuses a tree across restyles has to reset the pair itself. Skipping this makes the next
    /// restyle inherit the previous one's snapshot.
    pub(crate) fn forget_snapshot(&self) {
        let element = self.element();
        element.has_snapshot.set(false);
        element.handled_snapshot.set(false);
    }

    /// Safe wrappers over the `unsafe fn` items `TElement` declares.
    ///
    /// Those signatures exist for Gecko's benefit and their bodies are ordinary safe code, but
    /// calling one still needs an `unsafe` block — and this module is the only place in the crate
    /// allowed to write one. Everything outside `css::stylo::dom` goes through these.
    pub(crate) fn allocate_data(&self) {
        unsafe { style::dom::TElement::ensure_data(self) };
    }

    pub(crate) fn release_data(&self) {
        unsafe { style::dom::TElement::clear_data(self) };
    }

    pub(crate) fn set_styles(&self, styles: style::data::ElementStyles) {
        unsafe { style::dom::TElement::ensure_data(self) }.styles = styles;
    }

    /// The `&mut ElementData` the traversal hands to `recalc_style_at`.
    pub(crate) fn ensure_style_data(&self) -> style::data::ElementDataMut<'_> {
        unsafe { style::dom::TElement::ensure_data(self) }
    }

    pub(crate) fn primary_style(&self) -> Option<ServoArc<ComputedValues>> {
        style::dom::TElement::borrow_data(self)
            .filter(|data| data.has_styles())
            .map(|data| data.styles.primary().clone())
    }

    pub(crate) fn pseudo_style(
        &self,
        pseudo: style::selector_parser::PseudoElement,
    ) -> Option<ServoArc<ComputedValues>> {
        style::dom::TElement::borrow_data(self)
            .and_then(|data| data.styles.pseudos.get(&pseudo).cloned())
    }
}

impl StyloDocument<'_> {
    fn document(&self) -> &DocumentNode {
        match &self.0.kind {
            NodeKind::Document(document) => document,
            _ => unreachable!("a StyloDocument is only built over the document node"),
        }
    }
}

/// Identity is the node's address in the arena, which never moves while the arena is alive.
macro_rules! node_identity {
    ($handle:ident) => {
        impl PartialEq for $handle<'_> {
            fn eq(&self, other: &Self) -> bool {
                std::ptr::eq(self.0, other.0)
            }
        }

        impl Eq for $handle<'_> {}

        impl std::hash::Hash for $handle<'_> {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                std::ptr::from_ref(self.0).hash(state);
            }
        }
    };
}

node_identity!(StyloNode);
node_identity!(StyloElement);
node_identity!(StyloDocument);

impl std::fmt::Debug for StyloNode<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0.kind {
            NodeKind::Document(_) => write!(formatter, "#document"),
            NodeKind::Text => write!(formatter, "#text"),
            NodeKind::Element(element) => write!(formatter, "<{}>", element.local_name),
        }
    }
}

impl std::fmt::Debug for StyloElement<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&StyloNode(self.0), formatter)
    }
}

impl std::fmt::Debug for StyloDocument<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "#document")
    }
}
