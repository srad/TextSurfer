use std::borrow::Borrow;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use cssparser::{Parser as CssTokenParser, ParserInput, ToCss, serialize_identifier};
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::matches_selector;
use selectors::parser::{
    NonTSPseudoClass, ParseRelative, Parser, PseudoElement, SelectorImpl, SelectorList,
    SelectorParseErrorKind,
};
use selectors::{Element, OpaqueElement};

use crate::core::dom::{AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Atom(String);

impl Atom {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Atom {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Atom {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl ToCss for Atom {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for Atom {
    fn precomputed_hash(&self) -> u32 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish() as u32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnsupportedPseudoClass {}

impl ToCss for UnsupportedPseudoClass {
    fn to_css<W>(&self, _dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        match *self {}
    }
}

impl NonTSPseudoClass for UnsupportedPseudoClass {
    type Impl = TextSurferSelectorImpl;

    fn is_active_or_hover(&self) -> bool {
        match *self {}
    }

    fn is_user_action_state(&self) -> bool {
        match *self {}
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnsupportedPseudoElement {}

impl ToCss for UnsupportedPseudoElement {
    fn to_css<W>(&self, _dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        match *self {}
    }
}

impl PseudoElement for UnsupportedPseudoElement {
    type Impl = TextSurferSelectorImpl;
}

#[derive(Clone, Debug)]
pub struct TextSurferSelectorImpl;

impl SelectorImpl for TextSurferSelectorImpl {
    type ExtraMatchingData<'a> = PhantomData<&'a ()>;
    type AttrValue = Atom;
    type Identifier = Atom;
    type LocalName = Atom;
    type NamespaceUrl = Atom;
    type NamespacePrefix = Atom;
    type BorrowedLocalName = str;
    type BorrowedNamespaceUrl = str;
    type NonTSPseudoClass = UnsupportedPseudoClass;
    type PseudoElement = UnsupportedPseudoElement;
}

#[derive(Default)]
pub struct SelectorParser;

impl<'i> Parser<'i> for SelectorParser {
    type Impl = TextSurferSelectorImpl;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_nth_child_of(&self) -> bool {
        true
    }
}

pub type ParsedSelectors = SelectorList<TextSurferSelectorImpl>;

pub fn parse(input: &str) -> Option<ParsedSelectors> {
    let mut input = ParserInput::new(input);
    SelectorList::parse(
        &SelectorParser,
        &mut CssTokenParser::new(&mut input),
        ParseRelative::No,
    )
    .ok()
}

pub fn matching_specificity(
    selectors: &ParsedSelectors,
    document: &Document,
    id: NodeId,
) -> Option<u32> {
    let element = DomElement { document, id };
    let mut caches = SelectorCaches::default();
    let quirks = match document.quirks_mode() {
        DomQuirksMode::Quirks => QuirksMode::Quirks,
        DomQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        DomQuirksMode::NoQuirks => QuirksMode::NoQuirks,
    };
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        quirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    selectors
        .slice()
        .iter()
        .filter(|selector| matches_selector(selector, 0, None, &element, &mut context))
        .map(|selector| selector.specificity())
        .max()
}

#[derive(Clone, Debug)]
struct DomElement<'a> {
    document: &'a Document,
    id: NodeId,
}

impl DomElement<'_> {
    fn element(&self) -> (&str, ElementNs, &[crate::core::dom::Attr]) {
        match self.document.node(self.id) {
            Some(Node::Element { name, ns, attrs }) => (name, *ns, attrs),
            _ => unreachable!(),
        }
    }

    fn sibling_element(&self, mut id: Option<NodeId>, next: bool) -> Option<Self> {
        while let Some(candidate) = id {
            if matches!(self.document.node(candidate), Some(Node::Element { .. })) {
                return Some(Self {
                    document: self.document,
                    id: candidate,
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
                    id: candidate,
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
        pseudo: &UnsupportedPseudoClass,
        _context: &mut MatchingContext<TextSurferSelectorImpl>,
    ) -> bool {
        match *pseudo {}
    }

    fn match_pseudo_element(
        &self,
        pseudo: &UnsupportedPseudoElement,
        _context: &mut MatchingContext<TextSurferSelectorImpl>,
    ) -> bool {
        match *pseudo {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::Attr;

    #[test]
    fn html_names_are_ascii_insensitive_but_foreign_names_are_not() {
        let mut document = Document::new();
        let html = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let svg = document.insert_element(
            None,
            "linearGradient",
            ElementNs::Svg,
            vec![Attr::plain("viewBox", "0 0 1 1")],
        );
        assert!(matching_specificity(&parse("DIV").unwrap(), &document, html).is_some());
        assert!(
            matching_specificity(&parse("linearGradient[viewBox]").unwrap(), &document, svg)
                .is_some()
        );
        assert!(matching_specificity(&parse("lineargradient").unwrap(), &document, svg).is_none());
        assert!(
            matching_specificity(&parse("linearGradient[viewbox]").unwrap(), &document, svg)
                .is_none()
        );
    }
}
