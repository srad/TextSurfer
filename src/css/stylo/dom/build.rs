use std::cell::Cell;
use std::collections::HashMap;

use selectors::matching::ElementSelectorFlags;
use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::data::ElementDataWrapper;
use style::properties::{PropertyDeclarationBlock, parse_style_attribute};
use style::shared_lock::{Locked, SharedRwLock};
use style::stylesheets::CssRuleType;
use style::values::{AtomIdent, AtomString};
use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace, ns};

use crate::core::dom::{Attr, AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};
use crate::core::form::{ControlKind, control_kind};
use crate::core::style::LegacyAlign;
use crate::css::presentational::synthesized_hints;
use crate::css::ua::inline_style;

use super::{DocumentNode, ElementNode, MirrorAttr, NodeKind, StyleArena, StyleDom, StyleNode};

/// Guards against a pathologically deep document exhausting the mirror build.
///
/// Stylo's own traversal is an iterative breadth-first walk, so it carries no stack risk, but this
/// builder descends the source arena and the source is untrusted.
pub(crate) const MAX_MIRROR_DEPTH: usize = 512;

/// What the whole build needs and one node does not: the source, the lock every `style` attribute is
/// wrapped in, the quirks mode they parse under, and the intern table that keeps identical ones
/// pointing at a single block.
struct Build<'d> {
    document: &'d Document,
    lock: SharedRwLock,
    quirks_mode: QuirksMode,
    inline: HashMap<&'d str, Option<ServoArc<Locked<PropertyDeclarationBlock>>>>,
    hints: HashMap<String, Option<ServoArc<Locked<PropertyDeclarationBlock>>>>,
}

impl<'d> Build<'d> {
    /// The declaration block for `source`'s `style` attribute, shared with every other element that
    /// declares the same text.
    ///
    /// Interning is not just an allocation saving. `StyleSource`'s equality and the rule tree's key
    /// are both `Arc::ptr_eq`, so a block per element is a rule node per element: on the committed
    /// live-page fixture that is 290 rule nodes where 41 will do, and the mapper's memo — which
    /// counts *distinct* computed styles — would report the difference as lost sharing. It also
    /// puts the style sharing cache's `have_same_style_attribute` on its pointer fast path instead
    /// of a declaration-by-declaration comparison.
    ///
    /// The shared block must never be mutated in place; a future inline-style mutation replaces the
    /// entry rather than editing it.
    fn inline_style(
        &mut self,
        source: NodeId,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        // `css::ua::inline_style` rather than a second attribute lookup, so the predicate cannot
        // drift from the one `BasicCascade` uses.
        let css = inline_style(self.document, source)?;
        if let Some(interned) = self.inline.get(css) {
            return interned.clone();
        }
        let block = parse_style_attribute(
            css,
            &super::super::sheets::base_url(),
            None,
            self.quirks_mode,
            CssRuleType::Style,
        );
        // An attribute that declares nothing is stored as nothing: it is what `BasicCascade` sees,
        // and it keeps the element shareable with its attribute-less siblings.
        let block = (!block.is_empty()).then(|| ServoArc::new(self.lock.wrap(block)));
        self.inline.insert(css, block.clone());
        block
    }

    fn presentational_hints(
        &mut self,
        source: NodeId,
    ) -> (
        Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
        Option<LegacyAlign>,
    ) {
        let hints = synthesized_hints(self.document, source);
        let legacy_align = hints.legacy_align;
        let css = hints
            .declarations
            .into_iter()
            .map(|hint| {
                let value = match (hint.name.as_str(), hint.value.as_str()) {
                    ("vertical-align", "top") => "text-top",
                    ("vertical-align", "bottom") => "text-bottom",
                    _ => hint.value.as_str(),
                };
                format!("{}:{value};", hint.name)
            })
            .collect::<String>();
        if css.is_empty() {
            return (None, legacy_align);
        }
        if let Some(interned) = self.hints.get(&css) {
            return (interned.clone(), legacy_align);
        }
        let block = parse_style_attribute(
            &css,
            &super::super::sheets::base_url(),
            None,
            self.quirks_mode,
            CssRuleType::Style,
        );
        let block = (!block.is_empty()).then(|| ServoArc::new(self.lock.wrap(block)));
        self.hints.insert(css, block.clone());
        (block, legacy_align)
    }
}

impl<'a> StyleDom<'a> {
    /// Mirror `document`, wrapping every `style` attribute in `lock`.
    ///
    /// `lock` must be the lock of the engine that will cascade this mirror, and the engine must
    /// already exist: [`super::super::engine::StyloEngine::mirror`] is the constructor that
    /// guarantees both, and the only one anything but a mirror-only test should call.
    pub(crate) fn build(
        arena: &'a StyleArena<'a>,
        document: &Document,
        lock: SharedRwLock,
    ) -> Self {
        let quirks_mode = quirks_mode(document.quirks_mode());
        let root: &'a StyleNode<'a> = arena.alloc(StyleNode {
            document: Cell::new(None),
            dom_id: None,
            parent: Cell::new(None),
            first_child: Cell::new(None),
            last_child: Cell::new(None),
            prev_sibling: Cell::new(None),
            next_sibling: Cell::new(None),
            kind: NodeKind::Document(DocumentNode {
                quirks_mode,
                lock: lock.clone(),
            }),
        });
        let mut dom = Self {
            document: root,
            by_dom_id: HashMap::new(),
            len: 1,
        };
        let mut build = Build {
            document,
            lock,
            quirks_mode,
            inline: HashMap::new(),
            hints: HashMap::new(),
        };
        for source in document.roots() {
            dom.append_subtree(arena, &mut build, *source, root, 0);
        }
        dom
    }

    fn append_subtree(
        &mut self,
        arena: &'a StyleArena<'a>,
        build: &mut Build<'_>,
        source: NodeId,
        parent: &'a StyleNode<'a>,
        depth: usize,
    ) {
        if depth > MAX_MIRROR_DEPTH {
            return;
        }
        // Held separately from `build`, which the element arm needs mutably for the intern table.
        let document = build.document;
        let Some(node) = document.node(source) else {
            return;
        };
        let kind = match node {
            Node::Element { name, ns, attrs } => {
                let (presentational_hints, legacy_align) = build.presentational_hints(source);
                NodeKind::Element(element_node(
                    name,
                    *ns,
                    attrs,
                    control_kind(document, source),
                    build.inline_style(source),
                    presentational_hints,
                    legacy_align,
                ))
            }
            Node::Text { .. } => NodeKind::Text,
            // Comments, PIs, doctypes and fragments never match a selector and never inherit, so
            // the mirror leaves them out entirely rather than carrying dead nodes through every
            // traversal.
            _ => return,
        };
        let mirrored = self.push(arena, source, parent, kind);
        for child in document.children(source) {
            self.append_subtree(arena, build, child, mirrored, depth + 1);
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

fn element_node(
    name: &str,
    ns: ElementNs,
    attrs: &[Attr],
    control: Option<ControlKind>,
    style_attribute: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
    presentational_hints: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,
    legacy_align: Option<LegacyAlign>,
) -> ElementNode {
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
        control,
        state: Cell::new(ElementState::empty()),
        selector_flags: Cell::new(ElementSelectorFlags::empty()),
        data: ElementDataWrapper::default(),
        allocated: Cell::new(false),
        style_attribute,
        presentational_hints,
        legacy_align,
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
