use crate::css::selectors::ParsedSelectors;

use super::diagnostics::CssDiagnostics;
use super::media::MediaQueryList;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

#[derive(Clone, Debug)]
pub struct StyleRule {
    pub selectors: ParsedSelectors,
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug)]
pub enum CssRule {
    Style(StyleRule),
    Media(MediaRule),
    Import(ImportRule),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRule {
    pub url: String,
    pub queries: MediaQueryList,
}

pub(super) fn rule_has_content(rule: &CssRule) -> bool {
    match rule {
        CssRule::Style(rule) => !rule.declarations.is_empty(),
        CssRule::Media(_) => true,
        CssRule::Import(_) => true,
    }
}

#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    pub rules: Vec<CssRule>,
    pub diagnostics: CssDiagnostics,
}

#[derive(Clone, Debug)]
pub struct MediaRule {
    pub queries: MediaQueryList,
    pub rules: Vec<CssRule>,
}
