use std::marker::PhantomData;

use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::MatchingContext;
use selectors::matching::{ElementSelectorFlags, VisitedHandlingMode};
use selectors::sink::Push;
use selectors::{Element as SelectorsElement, OpaqueElement};
use servo_arc::{Arc as ServoArc, ArcBorrow};
use style::applicable_declarations::ApplicableDeclarationBlock;
use style::context::{QuirksMode, SharedStyleContext};
use style::data::{ElementData, ElementDataMut, ElementDataRef};
use style::dom::{LayoutIterator, NodeInfo, OpaqueNode, TDocument, TElement, TNode, TShadowRoot};
use style::properties::PropertyDeclarationBlock;
use style::selector_parser::{AttrValue, Lang, NonTSPseudoClass, PseudoElement, SelectorImpl};
use style::shared_lock::{Locked, SharedRwLock};
use style::stylist::CascadeData;
use style::values::{AtomIdent, GenericAtomIdent};
use style::{Atom, CaseSensitivityExt, LocalName as StyleLocalName, Namespace as StyleNamespace};
use stylo_dom::ElementState;
use web_atoms::{LocalName, Namespace, ns};

use super::{DOCUMENT, MirrorId, NodeKind, StyleDom, StyloDocument, StyloElement, StyloNode};

/// The mirror has no shadow DOM. Stylo only ever reaches this type through `Option`, so an
/// uninhabited stand-in satisfies the associated type without admitting any shadow behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoShadowRoot<'dom> {
    // Only under `test`: the module-level expectation already covers the lib build.
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "uninhabited on purpose: carrying the lifetime is the variant's only job"
        )
    )]
    Never(PhantomData<&'dom ()>, std::convert::Infallible),
}

impl<'dom> TShadowRoot for NoShadowRoot<'dom> {
    type ConcreteNode = StyloNode<'dom>;

    fn as_node(&self) -> Self::ConcreteNode {
        match *self {
            NoShadowRoot::Never(_, never) => match never {},
        }
    }

    fn host(&self) -> StyloElement<'dom> {
        match *self {
            NoShadowRoot::Never(_, never) => match never {},
        }
    }

    fn style_data<'a>(&self) -> Option<&'a CascadeData>
    where
        Self: 'a,
    {
        match *self {
            NoShadowRoot::Never(_, never) => match never {},
        }
    }
}

impl NodeInfo for StyloNode<'_> {
    fn is_element(&self) -> bool {
        matches!(self.dom.node(self.id).kind, NodeKind::Element(_))
    }

    fn is_text_node(&self) -> bool {
        matches!(self.dom.node(self.id).kind, NodeKind::Text)
    }
}

impl<'dom> TDocument for StyloDocument<'dom> {
    type ConcreteNode = StyloNode<'dom>;

    fn as_node(&self) -> Self::ConcreteNode {
        StyloNode {
            dom: self.0,
            id: DOCUMENT,
        }
    }

    fn is_html_document(&self) -> bool {
        true
    }

    fn quirks_mode(&self) -> QuirksMode {
        self.0.quirks_mode()
    }

    fn shared_lock(&self) -> &SharedRwLock {
        self.0.shared_lock()
    }
}

impl<'dom> TNode for StyloNode<'dom> {
    type ConcreteElement = StyloElement<'dom>;
    type ConcreteDocument = StyloDocument<'dom>;
    type ConcreteShadowRoot = NoShadowRoot<'dom>;

    fn parent_node(&self) -> Option<Self> {
        self.relative(self.dom.node(self.id).parent)
    }

    fn first_child(&self) -> Option<Self> {
        self.relative(self.dom.node(self.id).first_child)
    }

    fn last_child(&self) -> Option<Self> {
        self.relative(self.dom.node(self.id).last_child)
    }

    fn prev_sibling(&self) -> Option<Self> {
        self.relative(self.dom.node(self.id).prev_sibling)
    }

    fn next_sibling(&self) -> Option<Self> {
        self.relative(self.dom.node(self.id).next_sibling)
    }

    fn owner_doc(&self) -> Self::ConcreteDocument {
        StyloDocument(self.dom)
    }

    fn is_in_document(&self) -> bool {
        true
    }

    fn traversal_parent(&self) -> Option<Self::ConcreteElement> {
        self.parent_node().and_then(|parent| parent.as_element())
    }

    fn opaque(&self) -> OpaqueNode {
        OpaqueNode(self.dom.node_address(self.id))
    }

    fn debug_id(self) -> usize {
        self.id as usize
    }

    fn as_element(&self) -> Option<Self::ConcreteElement> {
        StyloElement::new(self.dom, self.id)
    }

    fn as_document(&self) -> Option<Self::ConcreteDocument> {
        (self.id == DOCUMENT).then_some(StyloDocument(self.dom))
    }

    fn as_shadow_root(&self) -> Option<Self::ConcreteShadowRoot> {
        None
    }
}

pub(crate) struct ChildIterator<'dom> {
    dom: &'dom StyleDom,
    next: Option<MirrorId>,
}

impl<'dom> Iterator for ChildIterator<'dom> {
    type Item = StyloNode<'dom>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        self.next = self.dom.node(id).next_sibling;
        Some(StyloNode { dom: self.dom, id })
    }
}

impl SelectorsElement for StyloElement<'_> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.element())
    }

    fn parent_element(&self) -> Option<Self> {
        self.0.parent_node().and_then(|node| node.as_element())
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let mut sibling = self.0.prev_sibling();
        while let Some(node) = sibling {
            if let Some(element) = node.as_element() {
                return Some(element);
            }
            sibling = node.prev_sibling();
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let mut sibling = self.0.next_sibling();
        while let Some(node) = sibling {
            if let Some(element) = node.as_element() {
                return Some(element);
            }
            sibling = node.next_sibling();
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        let mut child = self.0.first_child();
        while let Some(node) = child {
            if let Some(element) = node.as_element() {
                return Some(element);
            }
            child = node.next_sibling();
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        self.element().namespace == ns!(html)
    }

    fn has_local_name(&self, name: &LocalName) -> bool {
        self.element().local_name == *name
    }

    fn has_namespace(&self, namespace: &Namespace) -> bool {
        self.element().namespace == *namespace
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let (this, that) = (self.element(), other.element());
        this.local_name == that.local_name && this.namespace == that.namespace
    }

    fn attr_matches(
        &self,
        namespace: &NamespaceConstraint<&StyleNamespace>,
        local_name: &StyleLocalName,
        operation: &AttrSelectorOperation<&AttrValue>,
    ) -> bool {
        self.element().attrs.iter().any(|attr| {
            attr.name == **local_name
                && match namespace {
                    NamespaceConstraint::Any => true,
                    NamespaceConstraint::Specific(namespace) => attr.namespace == ***namespace,
                }
                && operation.eval_str(&attr.value)
        })
    }

    fn has_custom_state(&self, _name: &AtomIdent) -> bool {
        false
    }

    fn match_non_ts_pseudo_class(
        &self,
        pseudo_class: &NonTSPseudoClass,
        _context: &mut MatchingContext<'_, Self::Impl>,
    ) -> bool {
        match pseudo_class {
            // `:visited` must never be observable to page styling, so the bit is never set and the
            // class never matches. Locked decision, ROADMAP.md.
            NonTSPseudoClass::Visited => false,
            NonTSPseudoClass::Link | NonTSPseudoClass::AnyLink => self.is_link(),
            NonTSPseudoClass::Lang(lang) => self.match_element_lang(None, lang),
            other => {
                let state = other.state_flag();
                !state.is_empty() && self.element().state.get().intersects(state)
            }
        }
    }

    fn match_pseudo_element(
        &self,
        _pseudo: &PseudoElement,
        _context: &mut MatchingContext<'_, Self::Impl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, flags: ElementSelectorFlags) {
        let element = self.element();
        element
            .selector_flags
            .set(element.selector_flags.get() | flags.for_self());
        if let Some(parent) = SelectorsElement::parent_element(self) {
            let parent = parent.element();
            parent
                .selector_flags
                .set(parent.selector_flags.get() | flags.for_parent());
        }
    }

    fn is_link(&self) -> bool {
        let element = self.element();
        matches!(element.local_name.as_ref(), "a" | "area" | "link")
            && element
                .attrs
                .iter()
                .any(|attr| attr.name.as_ref() == "href")
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        self.element()
            .id
            .as_ref()
            .is_some_and(|own| case_sensitivity.eq_atom(&own.0, &id.0))
    }

    fn has_class(&self, name: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        self.element()
            .classes
            .iter()
            .any(|class| case_sensitivity.eq_atom(&class.0, &name.0))
    }

    fn imported_part(&self, _name: &AtomIdent) -> Option<AtomIdent> {
        None
    }

    fn is_part(&self, _name: &AtomIdent) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let mut child = self.0.first_child();
        while let Some(node) = child {
            match node.dom.node(node.id).kind {
                NodeKind::Element(_) | NodeKind::Text => return false,
                NodeKind::Document => {}
            }
            child = node.next_sibling();
        }
        true
    }

    fn is_root(&self) -> bool {
        self.0.dom.root_element() == Some(self.0.id)
    }

    fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
        false
    }
}

impl<'dom> TElement for StyloElement<'dom> {
    type ConcreteNode = StyloNode<'dom>;
    type TraversalChildrenIterator = ChildIterator<'dom>;

    fn as_node(&self) -> Self::ConcreteNode {
        self.0
    }

    fn traversal_children(&self) -> LayoutIterator<Self::TraversalChildrenIterator> {
        LayoutIterator(ChildIterator {
            dom: self.0.dom,
            next: self.0.dom.node(self.0.id).first_child,
        })
    }

    fn is_html_element(&self) -> bool {
        self.element().namespace == ns!(html)
    }

    fn is_mathml_element(&self) -> bool {
        self.element().namespace == ns!(mathml)
    }

    fn is_svg_element(&self) -> bool {
        self.element().namespace == ns!(svg)
    }

    fn style_attribute(&self) -> Option<ArcBorrow<'_, Locked<PropertyDeclarationBlock>>> {
        self.element()
            .style_attribute
            .as_ref()
            .map(|block| block.borrow_arc())
    }

    fn animation_rule(
        &self,
        _context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn transition_rule(
        &self,
        _context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        None
    }

    fn state(&self) -> ElementState {
        self.element().state.get()
    }

    fn has_part_attr(&self) -> bool {
        false
    }

    fn exports_any_part(&self) -> bool {
        false
    }

    fn id(&self) -> Option<&Atom> {
        self.element().id.as_ref().map(|id| &id.0)
    }

    fn each_class<F>(&self, mut callback: F)
    where
        F: FnMut(&AtomIdent),
    {
        for class in &self.element().classes {
            callback(class);
        }
    }

    fn each_custom_state<F>(&self, _callback: F)
    where
        F: FnMut(&AtomIdent),
    {
    }

    fn each_attr_name<F>(&self, mut callback: F)
    where
        F: FnMut(&StyleLocalName),
    {
        for attr in &self.element().attrs {
            callback(&GenericAtomIdent(attr.name.clone()));
        }
    }

    fn has_dirty_descendants(&self) -> bool {
        self.element().dirty_descendants.get()
    }

    fn has_snapshot(&self) -> bool {
        self.element().has_snapshot.get()
    }

    fn handled_snapshot(&self) -> bool {
        self.element().handled_snapshot.get()
    }

    unsafe fn set_handled_snapshot(&self) {
        self.element().handled_snapshot.set(true);
    }

    unsafe fn set_dirty_descendants(&self) {
        self.element().dirty_descendants.set(true);
    }

    unsafe fn unset_dirty_descendants(&self) {
        self.element().dirty_descendants.set(false);
    }

    fn store_children_to_process(&self, n: isize) {
        self.element().children_to_process.set(n);
    }

    fn did_process_child(&self) -> isize {
        let element = self.element();
        let remaining = element.children_to_process.get() - 1;
        element.children_to_process.set(remaining);
        remaining
    }

    unsafe fn ensure_data(&self) -> ElementDataMut<'_> {
        self.element().data.borrow_mut()
    }

    unsafe fn clear_data(&self) {
        *self.element().data.borrow_mut() = ElementData::default();
    }

    fn has_data(&self) -> bool {
        true
    }

    fn borrow_data(&self) -> Option<ElementDataRef<'_>> {
        Some(self.element().data.borrow())
    }

    fn mutate_data(&self) -> Option<ElementDataMut<'_>> {
        Some(self.element().data.borrow_mut())
    }

    fn skip_item_display_fixup(&self) -> bool {
        false
    }

    fn may_have_animations(&self) -> bool {
        false
    }

    fn has_animations(&self, _context: &SharedStyleContext) -> bool {
        false
    }

    fn has_css_animations(
        &self,
        _context: &SharedStyleContext,
        _pseudo: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn has_css_transitions(
        &self,
        _context: &SharedStyleContext,
        _pseudo: Option<PseudoElement>,
    ) -> bool {
        false
    }

    fn shadow_root(&self) -> Option<NoShadowRoot<'dom>> {
        None
    }

    fn containing_shadow(&self) -> Option<NoShadowRoot<'dom>> {
        None
    }

    fn lang_attr(&self) -> Option<AttrValue> {
        self.attr(&ns!(), "lang").cloned()
    }

    fn match_element_lang(&self, override_lang: Option<Option<AttrValue>>, value: &Lang) -> bool {
        let lang = match override_lang {
            Some(lang) => lang,
            None => self.inherited_lang(),
        };
        lang.is_some_and(|lang| {
            lang.to_ascii_lowercase()
                .starts_with(&value.to_ascii_lowercase())
        })
    }

    fn is_html_document_body_element(&self) -> bool {
        self.element().local_name.as_ref() == "body"
            && SelectorsElement::parent_element(self)
                .is_some_and(|parent| parent.element().local_name.as_ref() == "html")
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        _visited_handling: VisitedHandlingMode,
        _hints: &mut V,
    ) where
        V: Push<ApplicableDeclarationBlock>,
    {
    }

    fn local_name(&self) -> &LocalName {
        &self.element().local_name
    }

    fn namespace(&self) -> &Namespace {
        &self.element().namespace
    }

    /// Container queries need a used box size, which only exists after layout. Reporting no size on
    /// either axis is the spec's "no size containment" answer and keeps `@container` inert until
    /// layout can feed it.
    fn query_container_size(
        &self,
        _display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        euclid::default::Size2D::new(None, None)
    }

    fn has_selector_flags(&self, flags: ElementSelectorFlags) -> bool {
        self.element().selector_flags.get().contains(flags)
    }

    fn relative_selector_search_direction(&self) -> ElementSelectorFlags {
        ElementSelectorFlags::empty()
    }

    fn get_attr(&self, attr: &StyleLocalName, namespace: &StyleNamespace) -> Option<String> {
        self.attr(namespace, attr.as_ref())
            .map(|value| value.to_string())
    }
}

impl<'dom> StyloElement<'dom> {
    fn attr(&self, namespace: &Namespace, name: &str) -> Option<&'dom AttrValue> {
        self.element()
            .attrs
            .iter()
            .find(|attr| attr.namespace == *namespace && attr.name.as_ref() == name)
            .map(|attr| &attr.value)
    }

    fn inherited_lang(&self) -> Option<AttrValue> {
        let mut element = Some(*self);
        while let Some(current) = element {
            if let Some(lang) = current.attr(&ns!(), "lang") {
                return Some(lang.clone());
            }
            element = SelectorsElement::parent_element(&current);
        }
        None
    }
}
