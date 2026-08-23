use std::borrow::Borrow;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use cssparser::{
    CowRcStr, ParseError, Parser as CssTokenParser, ParserInput, SourceLocation, ToCss,
    serialize_identifier,
};
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::matches_selector;
use selectors::parser::{
    Component, NonTSPseudoClass, ParseRelative, Parser, PseudoElement as PseudoElementTrait,
    Selector as ParsedSelector, SelectorImpl, SelectorList, SelectorParseErrorKind,
};
use selectors::{Element, OpaqueElement};

use crate::core::dom::{AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};
use crate::core::style::PseudoElement;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicPseudoClass {
    Link,
    AnyLink,
    Visited,
    Hover,
    Focus,
    Active,
    Checked,
    Enabled,
    Disabled,
}

impl DynamicPseudoClass {
    fn name(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::AnyLink => "any-link",
            Self::Visited => "visited",
            Self::Hover => "hover",
            Self::Focus => "focus",
            Self::Active => "active",
            Self::Checked => "checked",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "link" => Some(Self::Link),
            "any-link" => Some(Self::AnyLink),
            "visited" => Some(Self::Visited),
            "hover" => Some(Self::Hover),
            "focus" | "focus-visible" | "focus-within" => Some(Self::Focus),
            "active" => Some(Self::Active),
            "checked" => Some(Self::Checked),
            "enabled" => Some(Self::Enabled),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

impl ToCss for DynamicPseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        write!(dest, ":{}", self.name())
    }
}

impl NonTSPseudoClass for DynamicPseudoClass {
    type Impl = TextSurferSelectorImpl;

    fn is_active_or_hover(&self) -> bool {
        matches!(self, Self::Active | Self::Hover)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(self, Self::Active | Self::Hover | Self::Focus)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicState {
    pub hover: Option<NodeId>,
    pub focus: Option<NodeId>,
    pub active: Option<NodeId>,
}

impl DynamicState {
    pub const INERT: Self = Self {
        hover: None,
        focus: None,
        active: None,
    };
}

/// What a rule is being matched against: the element itself, or one of its pseudo-elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchTarget {
    #[default]
    Element,
    Pseudo(PseudoElement),
}

fn pseudo_element_name(pseudo: PseudoElement) -> &'static str {
    match pseudo {
        PseudoElement::Before => "before",
        PseudoElement::After => "after",
        PseudoElement::Marker => "marker",
    }
}

fn parse_pseudo_element_name(name: &str) -> Option<PseudoElement> {
    match name.to_ascii_lowercase().as_str() {
        "before" => Some(PseudoElement::Before),
        "after" => Some(PseudoElement::After),
        "marker" => Some(PseudoElement::Marker),
        _ => None,
    }
}

/// Newtype so the `selectors` crate's `PseudoElement` trait can be implemented for our own enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectorPseudoElement(pub PseudoElement);

impl ToCss for SelectorPseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        write!(dest, "::{}", pseudo_element_name(self.0))
    }
}

impl PseudoElementTrait for SelectorPseudoElement {
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
    type NonTSPseudoClass = DynamicPseudoClass;
    type PseudoElement = SelectorPseudoElement;
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

    fn parse_non_ts_pseudo_class(
        &self,
        location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<DynamicPseudoClass, ParseError<'i, Self::Error>> {
        DynamicPseudoClass::parse(&name).ok_or_else(|| {
            location.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                name,
            ))
        })
    }

    fn parse_pseudo_element(
        &self,
        location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<SelectorPseudoElement, ParseError<'i, Self::Error>> {
        parse_pseudo_element_name(&name)
            .map(SelectorPseudoElement)
            .ok_or_else(|| {
                location.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                    name,
                ))
            })
    }
}

pub type ParsedSelectors = SelectorList<TextSurferSelectorImpl>;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum BucketKey {
    Id(String),
    Class(String),
    LocalName(String),
    Universal,
}

pub(crate) fn bucket_keys(selectors: &ParsedSelectors, quirks: bool) -> Vec<BucketKey> {
    let mut keys = Vec::new();
    for selector in selectors.slice() {
        let mut id = None;
        let mut class = None;
        let mut local_names = Vec::new();
        for component in selector.iter() {
            match component {
                Component::ID(value) if id.is_none() => {
                    id = Some(normalize_bucket(value.as_str(), quirks));
                }
                Component::Class(value) if class.is_none() => {
                    class = Some(normalize_bucket(value.as_str(), quirks));
                }
                Component::LocalName(value) if local_names.is_empty() => {
                    local_names.push(value.name.as_str().to_string());
                    if value.lower_name != value.name {
                        local_names.push(value.lower_name.as_str().to_string());
                    }
                }
                _ => {}
            }
        }
        if let Some(id) = id {
            keys.push(BucketKey::Id(id));
        } else if let Some(class) = class {
            keys.push(BucketKey::Class(class));
        } else if !local_names.is_empty() {
            keys.extend(local_names.into_iter().map(BucketKey::LocalName));
        } else {
            keys.push(BucketKey::Universal);
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

fn normalize_bucket(value: &str, quirks: bool) -> String {
    if quirks {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

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
    state: DynamicState,
    target: MatchTarget,
) -> Option<u32> {
    let element = DomElement {
        document,
        id,
        state,
    };
    let mut caches = SelectorCaches::default();
    let quirks = match document.quirks_mode() {
        DomQuirksMode::Quirks => QuirksMode::Quirks,
        DomQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        DomQuirksMode::NoQuirks => QuirksMode::NoQuirks,
    };
    let mode = match target {
        MatchTarget::Element => MatchingMode::Normal,
        MatchTarget::Pseudo(_) => MatchingMode::ForStatelessPseudoElement,
    };
    let mut context = MatchingContext::new(
        mode,
        None,
        &mut caches,
        quirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    selectors
        .slice()
        .iter()
        .filter(|selector| selector_targets(selector, target))
        .filter(|selector| matches_selector(selector, 0, None, &element, &mut context))
        .map(|selector| selector.specificity())
        .max()
}

/// `ForStatelessPseudoElement` matching assumes the caller has already checked that the selector
/// ends in the pseudo-element being matched, so filter the list before handing it to the crate.
fn selector_targets(
    selector: &ParsedSelector<TextSurferSelectorImpl>,
    target: MatchTarget,
) -> bool {
    match (selector.pseudo_element(), target) {
        (None, MatchTarget::Element) => true,
        (Some(pseudo), MatchTarget::Pseudo(wanted)) => pseudo.0 == wanted,
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct DomElement<'a> {
    document: &'a Document,
    id: NodeId,
    state: DynamicState,
}

impl DomElement<'_> {
    fn element(&self) -> (&str, ElementNs, &[crate::core::dom::Attr]) {
        match self.document.node(self.id) {
            Some(Node::Element { name, ns, attrs }) => (name, *ns, attrs),
            _ => unreachable!(),
        }
    }

    fn has_attribute(&self, name: &str) -> bool {
        let (_, _, attrs) = self.element();
        attrs
            .iter()
            .any(|attr| attr.ns == AttrNs::None && attr.name == name)
    }

    fn is_form_control(&self) -> bool {
        let (name, ns, _) = self.element();
        ns == ElementNs::Html
            && matches!(
                name,
                "input" | "button" | "select" | "textarea" | "option" | "optgroup" | "fieldset"
            )
    }

    fn is_selected(&self) -> bool {
        let (name, ns, _) = self.element();
        ns == ElementNs::Html && name == "option" && self.has_attribute("selected")
    }

    fn sibling_element(&self, mut id: Option<NodeId>, next: bool) -> Option<Self> {
        while let Some(candidate) = id {
            if matches!(self.document.node(candidate), Some(Node::Element { .. })) {
                return Some(Self {
                    document: self.document,
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
            DynamicPseudoClass::Hover => self.state.hover == Some(self.id),
            DynamicPseudoClass::Focus => self.state.focus == Some(self.id),
            DynamicPseudoClass::Active => self.state.active == Some(self.id),
            DynamicPseudoClass::Checked => self.has_attribute("checked") || self.is_selected(),
            DynamicPseudoClass::Disabled => {
                self.is_form_control() && self.has_attribute("disabled")
            }
            DynamicPseudoClass::Enabled => {
                self.is_form_control() && !self.has_attribute("disabled")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::Attr;

    fn matches(selector: &str, document: &Document, id: NodeId, state: DynamicState) -> bool {
        matches_target(selector, document, id, state, MatchTarget::Element)
    }

    fn matches_target(
        selector: &str,
        document: &Document,
        id: NodeId,
        state: DynamicState,
        target: MatchTarget,
    ) -> bool {
        matching_specificity(
            &parse(selector).expect("selector parses"),
            document,
            id,
            state,
            target,
        )
        .is_some()
    }

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
        let state = DynamicState::default();
        assert!(matches("DIV", &document, html, state));
        assert!(matches("linearGradient[viewBox]", &document, svg, state));
        assert!(!matches("lineargradient", &document, svg, state));
        assert!(!matches("linearGradient[viewbox]", &document, svg, state));
    }

    #[test]
    fn dynamic_pseudo_classes_parse_instead_of_invalidating_the_whole_selector_list() {
        let mut document = Document::new();
        let link = document.insert_element(
            None,
            "a",
            ElementNs::Html,
            vec![Attr::plain("href", "https://example.com/")],
        );
        let anchor = document.insert_element(None, "a", ElementNs::Html, vec![]);
        let state = DynamicState::default();
        assert!(matches("a:link", &document, link, state));
        assert!(matches("a:any-link", &document, link, state));
        assert!(!matches("a:link", &document, anchor, state));
        assert!(
            parse("p, a:hover").is_some(),
            "one dynamic class must not invalidate the whole selector list"
        );
    }

    #[test]
    fn visited_never_matches_so_page_styling_cannot_observe_history() {
        let mut document = Document::new();
        let link = document.insert_element(
            None,
            "a",
            ElementNs::Html,
            vec![Attr::plain("href", "https://example.com/")],
        );
        assert!(parse("a:visited").is_some());
        assert!(!matches(
            "a:visited",
            &document,
            link,
            DynamicState::default()
        ));
    }

    #[test]
    fn user_action_pseudo_classes_follow_the_injected_state() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        assert!(!matches(
            "div:hover",
            &document,
            div,
            DynamicState::default()
        ));
        assert!(matches(
            "div:hover",
            &document,
            div,
            DynamicState {
                hover: Some(div),
                ..Default::default()
            }
        ));
        assert!(matches(
            "div:focus",
            &document,
            div,
            DynamicState {
                focus: Some(div),
                ..Default::default()
            }
        ));
    }

    #[test]
    fn form_state_pseudo_classes_read_the_attributes() {
        let mut document = Document::new();
        let checked = document.insert_element(
            None,
            "input",
            ElementNs::Html,
            vec![Attr::plain("checked", "")],
        );
        let disabled = document.insert_element(
            None,
            "input",
            ElementNs::Html,
            vec![Attr::plain("disabled", "")],
        );
        let plain = document.insert_element(None, "input", ElementNs::Html, vec![]);
        let paragraph = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let state = DynamicState::default();
        assert!(matches("input:checked", &document, checked, state));
        assert!(!matches("input:checked", &document, plain, state));
        assert!(matches("input:disabled", &document, disabled, state));
        assert!(matches("input:enabled", &document, plain, state));
        assert!(!matches("input:enabled", &document, disabled, state));
        assert!(!matches(":enabled", &document, paragraph, state));
    }
}
