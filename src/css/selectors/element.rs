use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::MatchingContext;
use selectors::{Element, OpaqueElement};

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::form::{FormState, checkedness, is_disabled};

use super::TextSurferSelectorImpl;
use super::atom::Atom;
use super::pseudo::{DynamicPseudoClass, DynamicState, SelectorPseudoElement};

#[derive(Clone, Debug)]
pub(super) struct DomElement<'a> {
    document: &'a Document,
    forms: &'a FormState,
    id: NodeId,
    state: DynamicState,
}

impl<'a> DomElement<'a> {
    pub(super) fn new(
        document: &'a Document,
        forms: &'a FormState,
        id: NodeId,
        state: DynamicState,
    ) -> Self {
        Self {
            document,
            forms,
            id,
            state,
        }
    }
}

impl DomElement<'_> {
    fn element(&self) -> (&str, ElementNs, &[crate::core::dom::Attr]) {
        match self.document.node(self.id) {
            Some(Node::Element { name, ns, attrs }) => (name, *ns, attrs),
            _ => unreachable!(),
        }
    }

    fn is_form_control(&self) -> bool {
        let (name, ns, _) = self.element();
        ns == ElementNs::Html
            && matches!(
                name,
                "input" | "button" | "select" | "textarea" | "option" | "optgroup" | "fieldset"
            )
    }

    /// The elements `:checked` can ever apply to: the two toggle input states, and `<option>`.
    fn is_checkable(&self) -> bool {
        let (name, ns, attrs) = self.element();
        if ns != ElementNs::Html {
            return false;
        }
        if name == "option" {
            return true;
        }
        name == "input"
            && attrs
                .iter()
                .find(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case("type"))
                .is_some_and(|attr| {
                    matches!(
                        attr.value.trim().to_ascii_lowercase().as_str(),
                        "checkbox" | "radio"
                    )
                })
    }

    fn is_text_entry_control(&self) -> bool {
        let (name, ns, attrs) = self.element();
        if ns != ElementNs::Html
            || attrs
                .iter()
                .any(|attr| attr.ns == AttrNs::None && attr.name == "disabled")
        {
            return false;
        }
        if name == "textarea" {
            return true;
        }
        if name != "input" {
            return false;
        }
        attrs
            .iter()
            .find(|attr| attr.ns == AttrNs::None && attr.name == "type")
            .is_none_or(|attr| {
                matches!(
                    attr.value.to_ascii_lowercase().as_str(),
                    "text" | "search" | "url" | "tel" | "email" | "password" | "number"
                )
            })
    }

    fn is_on_chain(&self, mut node: Option<NodeId>) -> bool {
        while let Some(candidate) = node {
            if candidate == self.id {
                return true;
            }
            node = self.document.parent(candidate);
        }
        false
    }

    fn sibling_element(&self, mut id: Option<NodeId>, next: bool) -> Option<Self> {
        while let Some(candidate) = id {
            if matches!(self.document.node(candidate), Some(Node::Element { .. })) {
                return Some(Self {
                    document: self.document,
                    forms: self.forms,
                    id: candidate,
                    state: self.state,
                });
            }
            id = if next {
                self.document.next_sibling(candidate)
            } else {
                self.document.prev_sibling(candidate)
            };
        }
        None
    }
}

impl Element for DomElement<'_> {
    type Impl = TextSurferSelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.document.node(self.id).expect("element node"))
    }

    fn parent_element(&self) -> Option<Self> {
        let mut parent = self.document.parent(self.id);
        while let Some(candidate) = parent {
            if matches!(self.document.node(candidate), Some(Node::Element { .. })) {
                return Some(Self {
                    document: self.document,
                    forms: self.forms,
                    id: candidate,
                    state: self.state,
                });
            }
            parent = self.document.parent(candidate);
        }
        None
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
        self.sibling_element(self.document.prev_sibling(self.id), false)
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.sibling_element(self.document.next_sibling(self.id), true)
    }

    fn first_element_child(&self) -> Option<Self> {
        self.sibling_element(self.document.first_child(self.id), true)
    }

    fn is_html_element_in_html_document(&self) -> bool {
        self.element().1 == ElementNs::Html
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        let (name, ns, _) = self.element();
        if ns == ElementNs::Html {
            name.eq_ignore_ascii_case(local_name)
        } else {
            name == local_name
        }
    }

    fn has_namespace(&self, ns: &str) -> bool {
        element_namespace(self.element().1) == ns
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let this = self.element();
        let other = other.element();
        this.1 == other.1
            && if this.1 == ElementNs::Html {
                this.0.eq_ignore_ascii_case(other.0)
            } else {
                this.0 == other.0
            }
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&Atom>,
        local_name: &Atom,
        operation: &AttrSelectorOperation<&Atom>,
    ) -> bool {
        let element_ns = self.element().1;
        self.element().2.iter().any(|attr| {
            let namespace_matches = match ns {
                NamespaceConstraint::Any => true,
                NamespaceConstraint::Specific(expected) => {
                    attr_namespace(attr.ns) == expected.as_str()
                }
            };
            namespace_matches
                && if element_ns == ElementNs::Html {
                    attr.name.eq_ignore_ascii_case(local_name.as_str())
                } else {
                    attr.name == local_name.as_str()
                }
                && operation.eval_str(&attr.value)
        })
    }

    fn match_non_ts_pseudo_class(
        &self,
        pseudo: &DynamicPseudoClass,
        _context: &mut MatchingContext<TextSurferSelectorImpl>,
    ) -> bool {
        match pseudo {
            DynamicPseudoClass::Link | DynamicPseudoClass::AnyLink => self.is_link(),
            DynamicPseudoClass::Visited => false,
            DynamicPseudoClass::Hover => self.is_on_chain(self.state.hover),
            DynamicPseudoClass::Focus => {
                self.state.focus.is_some_and(|focus| focus.node == self.id)
            }
            DynamicPseudoClass::FocusVisible => self.state.focus.is_some_and(|focus| {
                focus.node == self.id
                    && (focus.source == super::pseudo::FocusSource::Keyboard
                        || self.is_text_entry_control())
            }),
            DynamicPseudoClass::FocusWithin => {
                self.is_on_chain(self.state.focus.map(|focus| focus.node))
            }
            DynamicPseudoClass::Active => self.is_on_chain(self.state.active),
            // `:checked` is a host-language question, not an attribute question. Testing the
            // attribute on any element let `<div checked>` match and hide from a dump fixture.
            DynamicPseudoClass::Checked => {
                self.is_checkable() && checkedness(self.document, self.id, self.forms)
            }
            // "Actually disabled" reaches through a disabled `<fieldset>` or `<optgroup>`, so the
            // attribute alone is not the answer either.
            DynamicPseudoClass::Disabled => {
                self.is_form_control() && is_disabled(self.document, self.id)
            }
            DynamicPseudoClass::Enabled => {
                self.is_form_control() && !is_disabled(self.document, self.id)
            }
        }
    }

    fn match_pseudo_element(
        &self,
        _pseudo: &SelectorPseudoElement,
        _context: &mut MatchingContext<TextSurferSelectorImpl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: selectors::matching::ElementSelectorFlags) {}

    fn is_link(&self) -> bool {
        let (name, ns, attrs) = self.element();
        ns == ElementNs::Html
            && name == "a"
            && attrs
                .iter()
                .any(|attr| attr.ns == AttrNs::None && attr.name == "href")
    }

    fn is_html_slot_element(&self) -> bool {
        let (name, ns, _) = self.element();
        ns == ElementNs::Html && name == "slot"
    }

    fn has_id(&self, id: &Atom, case_sensitivity: CaseSensitivity) -> bool {
        self.element().2.iter().any(|attr| {
            attr.ns == AttrNs::None
                && attr.name == "id"
                && case_sensitivity.eq(attr.value.as_bytes(), id.as_str().as_bytes())
        })
    }

    fn has_class(&self, name: &Atom, case_sensitivity: CaseSensitivity) -> bool {
        self.element().2.iter().any(|attr| {
            attr.ns == AttrNs::None
                && attr.name == "class"
                && attr
                    .value
                    .split_ascii_whitespace()
                    .any(|class| case_sensitivity.eq(class.as_bytes(), name.as_str().as_bytes()))
        })
    }

    fn has_custom_state(&self, _name: &Atom) -> bool {
        false
    }

    fn imported_part(&self, _name: &Atom) -> Option<Atom> {
        None
    }

    fn is_part(&self, _name: &Atom) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.document
            .children(self.id)
            .iter()
            .all(|child| match self.document.node(*child) {
                Some(Node::Text { data }) => data.is_empty(),
                Some(Node::Comment { .. }) => true,
                Some(Node::Element { .. }) => false,
                _ => true,
            })
    }

    fn is_root(&self) -> bool {
        self.document.parent(self.id).is_none() && self.document.roots().contains(&self.id)
    }

    fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
        false
    }
}

fn element_namespace(ns: ElementNs) -> &'static str {
    match ns {
        ElementNs::Html => "http://www.w3.org/1999/xhtml",
        ElementNs::Svg => "http://www.w3.org/2000/svg",
        ElementNs::MathMl => "http://www.w3.org/1998/Math/MathML",
        ElementNs::Other => "",
    }
}

fn attr_namespace(ns: AttrNs) -> &'static str {
    match ns {
        AttrNs::None => "",
        AttrNs::Xlink => "http://www.w3.org/1999/xlink",
        AttrNs::Xml => "http://www.w3.org/XML/1998/namespace",
        AttrNs::Xmlns => "http://www.w3.org/2000/xmlns/",
    }
}
