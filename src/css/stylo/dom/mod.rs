#![allow(unsafe_code)]

mod build;
mod traits;

#[cfg(test)]
mod tests;

use std::cell::Cell;

use selectors::matching::ElementSelectorFlags;
use servo_arc::Arc as ServoArc;
use style::data::ElementDataWrapper;
use style::properties::PropertyDeclarationBlock;
use style::shared_lock::{Locked, SharedRwLock};
use style::values::{AtomIdent, AtomString};
use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace};

use crate::core::dom::NodeId;

pub(crate) use build::QuirksMode;

/// Index into [`StyleDom::nodes`]. Slot 0 is always the synthetic document node.
pub(crate) type MirrorId = u32;

pub(crate) const DOCUMENT: MirrorId = 0;

/// An owned mirror of the parts of `core::dom::Document` that Stylo needs to see.
///
/// Stylo requires interior-mutable per-element style data, state bits, selector flags and a stable
/// identity for every element. None of that can live on `core::dom::Node` without pulling `style::`
/// into `core`, so the style engine gets its own arena instead. Ids map back to the real document
/// through [`StyleNode::dom_id`].
pub(crate) struct StyleDom {
    nodes: Vec<StyleNode>,
    dom_to_mirror: std::collections::HashMap<NodeId, MirrorId>,
    quirks_mode: QuirksMode,
    lock: SharedRwLock,
}

pub(crate) struct StyleNode {
    dom_id: Option<NodeId>,
    parent: Option<MirrorId>,
    first_child: Option<MirrorId>,
    last_child: Option<MirrorId>,
    prev_sibling: Option<MirrorId>,
    next_sibling: Option<MirrorId>,
    kind: NodeKind,
}

pub(crate) enum NodeKind {
    Document,
    Text,
    Element(ElementNode),
}

pub(crate) struct ElementNode {
    local_name: LocalName,
    namespace: Namespace,
    id: Option<AtomIdent>,
    classes: Vec<AtomIdent>,
    attrs: Vec<MirrorAttr>,
    state: Cell<ElementState>,
    selector_flags: Cell<ElementSelectorFlags>,
    data: ElementDataWrapper,
    style_attribute: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
    /// Set while Stylo is walking the tree; see `TElement::store_children_to_process`.
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

impl StyleDom {
    pub(crate) fn node(&self, id: MirrorId) -> &StyleNode {
        &self.nodes[id as usize]
    }

    pub(crate) fn element(&self, id: MirrorId) -> Option<&ElementNode> {
        match &self.node(id).kind {
            NodeKind::Element(element) => Some(element),
            _ => None,
        }
    }

    pub(crate) fn quirks_mode(&self) -> QuirksMode {
        self.quirks_mode
    }

    pub(crate) fn shared_lock(&self) -> &SharedRwLock {
        &self.lock
    }

    pub(crate) fn dom_id(&self, id: MirrorId) -> Option<NodeId> {
        self.node(id).dom_id
    }

    pub(crate) fn mirror_id(&self, id: NodeId) -> Option<MirrorId> {
        self.dom_to_mirror.get(&id).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// The document element — the first element child of the synthetic document node.
    pub(crate) fn root_element(&self) -> Option<MirrorId> {
        let mut child = self.node(DOCUMENT).first_child;
        while let Some(id) = child {
            if matches!(self.node(id).kind, NodeKind::Element(_)) {
                return Some(id);
            }
            child = self.node(id).next_sibling;
        }
        None
    }

    pub(crate) fn set_state(&self, id: MirrorId, state: ElementState) {
        if let Some(element) = self.element(id) {
            element.state.set(state);
        }
    }

    pub(crate) fn state(&self, id: MirrorId) -> ElementState {
        self.element(id)
            .map_or_else(ElementState::empty, |element| element.state.get())
    }

    /// A stable per-node address for `OpaqueNode`. The arena owns every node for the lifetime of
    /// the mirror, so the slot's address is unique and never moves while Stylo holds it.
    pub(crate) fn node_address(&self, id: MirrorId) -> usize {
        std::ptr::from_ref(self.node(id)) as usize
    }
}

/// A `Copy` handle into [`StyleDom`]. Stylo's traits are all implemented on handles like this one
/// rather than on the arena, because `TNode`/`TElement` require `Copy`.
#[derive(Clone, Copy)]
pub(crate) struct StyloNode<'dom> {
    pub(crate) dom: &'dom StyleDom,
    pub(crate) id: MirrorId,
}

#[derive(Clone, Copy)]
pub(crate) struct StyloElement<'dom>(pub(crate) StyloNode<'dom>);

#[derive(Clone, Copy)]
pub(crate) struct StyloDocument<'dom>(pub(crate) &'dom StyleDom);

impl<'dom> StyloNode<'dom> {
    fn node(&self) -> &'dom StyleNode {
        self.dom.node(self.id)
    }

    pub(crate) fn relative(&self, id: Option<MirrorId>) -> Option<Self> {
        id.map(|id| Self { dom: self.dom, id })
    }
}

impl<'dom> StyloElement<'dom> {
    pub(crate) fn new(dom: &'dom StyleDom, id: MirrorId) -> Option<Self> {
        dom.element(id).map(|_| Self(StyloNode { dom, id }))
    }

    fn element(&self) -> &'dom ElementNode {
        self.0
            .dom
            .element(self.0.id)
            .expect("a StyloElement is only built over an element node")
    }

    pub(crate) fn dom_id(&self) -> Option<NodeId> {
        self.0.dom.dom_id(self.0.id)
    }
}

impl PartialEq for StyloNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.dom, other.dom) && self.id == other.id
    }
}

impl Eq for StyloNode<'_> {}

impl std::hash::Hash for StyloNode<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.dom as *const StyleDom).hash(state);
        self.id.hash(state);
    }
}

impl PartialEq for StyloElement<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for StyloElement<'_> {}

impl std::hash::Hash for StyloElement<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for StyloDocument<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for StyloDocument<'_> {}

impl std::fmt::Debug for StyloNode<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.node().kind {
            NodeKind::Document => write!(formatter, "#document"),
            NodeKind::Text => write!(formatter, "#text({})", self.id),
            NodeKind::Element(element) => {
                write!(formatter, "<{}>({})", element.local_name, self.id)
            }
        }
    }
}

impl std::fmt::Debug for StyloElement<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, formatter)
    }
}

impl std::fmt::Debug for StyloDocument<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "#document({} nodes)", self.0.len())
    }
}
