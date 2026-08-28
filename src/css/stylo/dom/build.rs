use std::cell::Cell;
use std::collections::HashMap;

use selectors::matching::ElementSelectorFlags;
use style::context::QuirksMode;
use style::data::ElementDataWrapper;
use style::shared_lock::SharedRwLock;
use style::values::{AtomIdent, AtomString};
use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace, ns};

use crate::core::dom::{Attr, AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};

use super::{DocumentNode, ElementNode, MirrorAttr, NodeKind, StyleArena, StyleDom, StyleNode};

/// Guards against a pathologically deep document exhausting the mirror build.
///
/// Stylo's own traversal is an iterative breadth-first walk, so it carries no stack risk, but this
/// builder descends the source arena and the source is untrusted.
pub(crate) const MAX_MIRROR_DEPTH: usize = 512;

impl<'a> StyleDom<'a> {
    pub(crate) fn build(
        arena: &'a StyleArena<'a>,
        document: &Document,
        lock: SharedRwLock,
    ) -> Self {
        let root: &'a StyleNode<'a> = arena.alloc(StyleNode {
            document: Cell::new(None),
            dom_id: None,
            parent: Cell::new(None),
            first_child: Cell::new(None),
            last_child: Cell::new(None),
            prev_sibling: Cell::new(None),
            next_sibling: Cell::new(None),
            kind: NodeKind::Document(DocumentNode {
                quirks_mode: quirks_mode(document.quirks_mode()),
                lock,
            }),
        });
        let mut dom = Self {
            document: root,
            by_dom_id: HashMap::new(),
            len: 1,
        };
        for source in document.roots() {
            dom.append_subtree(arena, document, *source, root, 0);
        }
        dom
    }

    fn append_subtree(
        &mut self,
        arena: &'a StyleArena<'a>,
        document: &Document,
        source: NodeId,
        parent: &'a StyleNode<'a>,
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
        let mirrored = self.push(arena, source, parent, kind);
        for child in document.children(source) {
            self.append_subtree(arena, document, child, mirrored, depth + 1);
        }
    }

    fn push(
        &mut self,
        arena: &'a StyleArena<'a>,
        source: NodeId,
        parent: &'a StyleNode<'a>,
        kind: NodeKind,
    ) -> &'a StyleNode<'a> {
        let previous = parent.last_child.get();
        let node: &'a StyleNode<'a> = arena.alloc(StyleNode {
            document: Cell::new(Some(self.document)),
            dom_id: Some(source),
            parent: Cell::new(Some(parent)),
            first_child: Cell::new(None),
            last_child: Cell::new(None),
            prev_sibling: Cell::new(previous),
            next_sibling: Cell::new(None),
            kind,
        });
        match previous {
            Some(previous) => previous.next_sibling.set(Some(node)),
            None => parent.first_child.set(Some(node)),
        }
        parent.last_child.set(Some(node));
        self.by_dom_id.insert(source, node);
        self.len += 1;
        node
    }
}

impl super::DocumentNode {
    pub(super) fn quirks_mode(&self) -> QuirksMode {
        self.quirks_mode
    }

    pub(super) fn lock(&self) -> &SharedRwLock {
        &self.lock
    }
}

fn element_node(name: &str, ns: ElementNs, attrs: &[Attr]) -> ElementNode {
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
        allocated: Cell::new(false),
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
