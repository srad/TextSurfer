mod hints;
mod legacy;

#[cfg(test)]
mod tests;

use crate::core::style::LegacyAlign;

pub(in crate::css) struct HintDeclaration {
    pub(in crate::css) name: String,
    pub(in crate::css) value: String,
}

pub(in crate::css) struct PresentationalHints {
    pub(in crate::css) declarations: Vec<HintDeclaration>,
    pub(in crate::css) legacy_align: Option<LegacyAlign>,
}

pub(in crate::css) use hints::synthesized_hints;
