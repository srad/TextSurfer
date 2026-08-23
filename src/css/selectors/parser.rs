use std::marker::PhantomData;

use cssparser::{CowRcStr, ParseError, Parser as CssTokenParser, ParserInput, SourceLocation};
use selectors::parser::{
    ParseRelative, Parser, SelectorImpl, SelectorList, SelectorParseErrorKind,
};

use super::atom::Atom;
use super::pseudo::{DynamicPseudoClass, SelectorPseudoElement, parse_pseudo_element_name};

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
