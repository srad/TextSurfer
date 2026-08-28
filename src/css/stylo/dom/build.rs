use std::cell::Cell;

use selectors::matching::ElementSelectorFlags;
use style::data::ElementDataWrapper;
use style::shared_lock::SharedRwLock;
use style::values::{AtomIdent, AtomString};

use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace, ns};

use crate::core::dom::{AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};

use super::{DOCUMENT, ElementNode, MirrorAttr, MirrorId, NodeKind, StyleDom, StyleNode};

pub(crate) use style::context::QuirksMode;

/// Guards against a pathologically deep document exhausting the mirror build.
///
/// Stylo's own traversal is an iterative breadth-first walk, so it carries no stack risk, but this
/// builder descends the source arena and the source is untrusted.
pub(crate) const MAX_MIRROR_DEPTH: usize = 512;

impl StyleDom {
    pub(crate) fn build(document: &Document, lock: SharedRwLock) -> Self {
        let mut dom = Self {
            nodes: vec![StyleNode {
                dom_id: None,
                parent: None,
                first_child: None,
                last_child: None,
                prev_sibling: None,
                next_sibling: None,
                kind: NodeKind::Document,
            }],
            dom_to_mirror: std::collections::HashMap::new(),
            quirks_mode: quirks_mode(document.quirks_mode()),
            lock,
        };
        for root in document.roots() {
            dom.append_subtree(document, *root, DOCUMENT, 0);
        }
        dom
    }

    fn append_subtree(
        &mut self,
        document: &Document,
        source: NodeId,
        parent: MirrorId,
        depth: usize,
    ) {
        if depth > MAX_MIRROR_DEPTH {
            return;
        }
        let Some(node) = document.node(source) else {
            return;
        };
        let kind = match node {
            Node::Element { name, ns, attrs } => NodeKind::Element(element_node(name, *ns, attrs)),
            Node::Text { .. } => NodeKind::Text,
            // Comments, PIs, doctypes and fragments never match a selector and never inherit, so
            // the mirror leaves them out entirely rather than carrying dead nodes through every
            // traversal.
            _ => return,
        };
        let id = self.push(source, parent, kind);
        for child in document.children(source) {
            self.append_subtree(document, child, id, depth + 1);
        }
    }

    fn push(&mut self, source: NodeId, parent: MirrorId, kind: NodeKind) -> MirrorId {
        let id = self.nodes.len() as MirrorId;
        let previous = self.nodes[parent as usize].last_child;
        self.nodes.push(StyleNode {
            dom_id: Some(source),
            parent: Some(parent),
            first_child: None,
            last_child: None,
            prev_sibling: previous,
            next_sibling: None,
            kind,
        });
        match previous {
            Some(previous) => self.nodes[previous as usize].next_sibling = Some(id),
            None => self.nodes[parent as usize].first_child = Some(id),
        }
        self.nodes[parent as usize].last_child = Some(id);
        self.dom_to_mirror.insert(source, id);
        id
    }
}

fn element_node(name: &str, ns: ElementNs, attrs: &[crate::core::dom::Attr]) -> ElementNode {
    let mut id = None;
    let mut classes = Vec::new();
    let mut mirrored = Vec::with_capacity(attrs.len());
    for attr in attrs {
        if attr.ns == AttrNs::None {
            if attr.name.eq_ignore_ascii_case("id") {
                id = Some(AtomIdent::from(attr.value.as_str()));
            } else if attr.name.eq_ignore_ascii_case("class") {
                classes.extend(attr.value.split_ascii_whitespace().map(AtomIdent::from));
            }
        }
        mirrored.push(MirrorAttr {
            name: LocalName::from(attr.name.as_str()),
            namespace: attr_namespace(attr.ns),
            value: AtomString::from(attr.value.as_str()),
        });
    }
    ElementNode {
        local_name: LocalName::from(name),
        namespace: element_namespace(ns),
        id,
        classes,
        attrs: mirrored,
        state: Cell::new(ElementState::empty()),
        selector_flags: Cell::new(ElementSelectorFlags::empty()),
        data: ElementDataWrapper::default(),
        style_attribute: None,
        children_to_process: Cell::new(0),
        dirty_descendants: Cell::new(false),
        has_snapshot: Cell::new(false),
        handled_snapshot: Cell::new(false),
    }
}

fn element_namespace(ns: ElementNs) -> Namespace {
    match ns {
        ElementNs::Html => ns!(html),
        ElementNs::Svg => ns!(svg),
        ElementNs::MathMl => ns!(mathml),
        ElementNs::Other => ns!(),
    }
}

fn attr_namespace(ns: AttrNs) -> Namespace {
    match ns {
        AttrNs::None => ns!(),
        AttrNs::Xlink => ns!(xlink),
        AttrNs::Xml => ns!(xml),
        AttrNs::Xmlns => ns!(xmlns),
    }
}

fn quirks_mode(mode: DomQuirksMode) -> QuirksMode {
    match mode {
        DomQuirksMode::Quirks => QuirksMode::Quirks,
        DomQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        DomQuirksMode::NoQuirks => QuirksMode::NoQuirks,
    }
}
