use selectors::parser::{Component, SelectorVisitor};

use super::parser::{ParsedSelectors, TextSurferSelectorImpl};
use super::pseudo::DynamicPseudoClass;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateDeps {
    pub hover: bool,
    pub focus: bool,
    pub active: bool,
}

impl StateDeps {
    pub const fn union(self, other: Self) -> Self {
        Self {
            hover: self.hover || other.hover,
            focus: self.focus || other.focus,
            active: self.active || other.active,
        }
    }
}

#[derive(Default)]
struct DependencyVisitor {
    deps: StateDeps,
}

impl SelectorVisitor for DependencyVisitor {
    type Impl = TextSurferSelectorImpl;

    fn visit_simple_selector(&mut self, selector: &Component<Self::Impl>) -> bool {
        if let Component::NonTSPseudoClass(pseudo) = selector {
            match pseudo {
                DynamicPseudoClass::Hover => self.deps.hover = true,
                DynamicPseudoClass::Focus
                | DynamicPseudoClass::FocusVisible
                | DynamicPseudoClass::FocusWithin => self.deps.focus = true,
                DynamicPseudoClass::Active => self.deps.active = true,
                _ => {}
            }
        }
        true
    }
}

pub(crate) fn uses_dynamic_state(selectors: &ParsedSelectors) -> StateDeps {
    let mut visitor = DependencyVisitor::default();
    for selector in selectors.slice() {
        selector.visit(&mut visitor);
    }
    visitor.deps
}
