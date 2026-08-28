use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::matches_selector;
use selectors::parser::Selector as ParsedSelector;

use crate::core::dom::{Document, DomQuirksMode, NodeId};
use crate::core::form::FormState;

use super::element::DomElement;
use super::parser::{ParsedSelectors, TextSurferSelectorImpl};
use super::pseudo::DynamicState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchTarget {
    #[default]
    Element,
    Pseudo(crate::core::style::PseudoElement),
}

#[cfg(test)]
pub fn matching_specificity(
    selectors: &ParsedSelectors,
    document: &Document,
    id: NodeId,
    state: DynamicState,
    target: MatchTarget,
) -> Option<u32> {
    matching_specificity_with_forms(selectors, document, FormState::empty(), id, state, target)
}

pub fn matching_specificity_with_forms(
    selectors: &ParsedSelectors,
    document: &Document,
    forms: &FormState,
    id: NodeId,
    state: DynamicState,
    target: MatchTarget,
) -> Option<u32> {
    let element = DomElement::new(document, forms, id, state);
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
