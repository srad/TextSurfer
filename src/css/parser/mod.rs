mod ast;
mod declarations;
mod diagnostics;
mod media;
mod sheet;

#[cfg(test)]
mod tests;

use cssparser::{Parser, ParserInput, StyleSheetParser};

use crate::css::selectors::{StateDeps, uses_dynamic_state};
use ast::rule_has_content;
use sheet::SheetParser;

pub use ast::{CssRule, Declaration, ImportRule, MediaRule, StyleRule, StyleSheet};
pub use declarations::parse_declarations;
pub use diagnostics::{CssDiagnostic, CssDiagnosticKind, CssDiagnostics, CssSourcePosition};
pub use media::{
    ColorScheme, DimensionCondition, MediaAxis, MediaBound, MediaComparison, MediaFeature,
    MediaQuery, MediaQueryList, ScriptingValue, parse_media_queries,
};

pub trait CssParser: Send + Sync {
    fn parse(&self, source: &str) -> StyleSheet;
}

#[derive(Default)]
pub struct CssparserParser;

impl CssParser for CssparserParser {
    fn parse(&self, source: &str) -> StyleSheet {
        let mut input = ParserInput::new(source);
        let mut input = Parser::new(&mut input);
        let mut diagnostics = CssDiagnostics::default();
        let mut parser = SheetParser::new(&mut diagnostics);
        let rules: Vec<CssRule> = StyleSheetParser::new(&mut input, &mut parser)
            .filter_map(Result::ok)
            .filter(rule_has_content)
            .collect();
        let state_deps = rules.iter().fold(StateDeps::default(), |deps, rule| {
            deps.union(rule_state_deps(rule))
        });
        StyleSheet {
            rules,
            diagnostics,
            state_deps,
        }
    }
}

fn rule_state_deps(rule: &CssRule) -> StateDeps {
    match rule {
        CssRule::Style(rule) => uses_dynamic_state(&rule.selectors),
        CssRule::Media(rule) => rule.rules.iter().fold(StateDeps::default(), |deps, rule| {
            deps.union(rule_state_deps(rule))
        }),
        CssRule::Import(_) => StateDeps::default(),
    }
}
