mod atom;
mod buckets;
mod element;
mod pseudo;

#[cfg(test)]
mod tests;

use std::marker::PhantomData;

use cssparser::{CowRcStr, ParseError, Parser as CssTokenParser, ParserInput, SourceLocation};
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::matches_selector;
use selectors::parser::{
    ParseRelative, Parser, Selector as ParsedSelector, SelectorImpl, SelectorList,
    SelectorParseErrorKind,
};

use crate::core::dom::{Document, DomQuirksMode, NodeId};

use element::DomElement;
use pseudo::parse_pseudo_element_name;

pub use atom::Atom;
pub(crate) use buckets::{BucketKey, bucket_keys};
pub use pseudo::{DynamicPseudoClass, DynamicState, SelectorPseudoElement};

/// What a rule is being matched against: the element itself, or one of its pseudo-elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchTarget {
    #[default]
    Element,
    Pseudo(crate::core::style::PseudoElement),
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
    let element = DomElement::new(document, id, state);
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
