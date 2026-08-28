use super::StyleSheet;
use super::cascade::{MediaContext, active_style_rules, is_supported_property};
use super::selectors::{DynamicState, uses_dynamic_state};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DynamicEffect {
    #[default]
    None,
    Paint,
    Layout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StateEffects {
    hover: DynamicEffect,
    focus: DynamicEffect,
    active: DynamicEffect,
}

impl StateEffects {
    pub(crate) fn for_sheets(sheets: &[StyleSheet], media: MediaContext) -> Self {
        let mut effects = Self::default();
        for rule in active_style_rules(sheets, media) {
            let deps = uses_dynamic_state(&rule.selectors);
            let effect = rule
                .declarations
                .iter()
                .map(|declaration| property_effect(&declaration.name))
                .max()
                .unwrap_or_default();
            if deps.hover {
                effects.hover = effects.hover.max(effect);
            }
            if deps.focus {
                effects.focus = effects.focus.max(effect);
            }
            if deps.active {
                effects.active = effects.active.max(effect);
            }
        }
        effects
    }

    pub(crate) fn transition(self, previous: DynamicState, next: DynamicState) -> DynamicEffect {
        let mut effect = DynamicEffect::None;
        if previous.hover != next.hover {
            effect = effect.max(self.hover);
        }
        if previous.focus != next.focus {
            effect = effect.max(self.focus);
        }
        if previous.active != next.active {
            effect = effect.max(self.active);
        }
        effect
    }
}

pub(crate) fn property_effect(property: &str) -> DynamicEffect {
    if matches!(
        property,
        "color"
            | "background"
            | "background-color"
            | "font-weight"
            | "text-decoration"
            | "text-decoration-line"
            | "border-color"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
            | "cursor"
    ) {
        DynamicEffect::Paint
    } else if property.starts_with("--")
        || matches!(
            property,
            "font-size" | "content" | "counter-reset" | "counter-increment"
        )
        || is_supported_property(property)
    {
        DynamicEffect::Layout
    } else {
        DynamicEffect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{CssParser, CssparserParser};

    fn effects(source: &str) -> StateEffects {
        StateEffects::for_sheets(&[CssparserParser.parse(source)], MediaContext::screen())
    }

    #[test]
    fn dynamic_effects_separate_paint_layout_and_unsupported_declarations() {
        assert_eq!(
            effects("a:hover { color: red }").hover,
            DynamicEffect::Paint
        );
        assert_eq!(
            effects("a:hover { width: 1px }").hover,
            DynamicEffect::Layout
        );
        assert_eq!(effects("a:hover { unknown: 1 }").hover, DynamicEffect::None);
        assert_eq!(
            effects("a:hover { color: red; width: 1px }").hover,
            DynamicEffect::Layout
        );
    }

    #[test]
    fn inactive_media_rules_do_not_raise_the_dynamic_effect() {
        assert_eq!(
            effects("@media print { a:hover { width: 1px } } a:hover { color: red }").hover,
            DynamicEffect::Paint
        );
    }
}
