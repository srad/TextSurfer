mod ast;
mod declarations;
mod diagnostics;
mod media;
mod sheet;

#[cfg(test)]
mod tests;

use cssparser::{Parser, ParserInput, StyleSheetParser};

use ast::rule_has_content;
use declarations::parse_declarations_from_parser;
use media::parse_media_query_list;
use sheet::SheetParser;

pub use ast::{CssRule, Declaration, ImportRule, MediaRule, StyleRule, StyleSheet};
pub use diagnostics::{CssDiagnostic, CssDiagnosticKind, CssDiagnostics, CssSourcePosition};
pub use media::{
    ColorScheme, MediaAxis, MediaComparison, MediaFeature, MediaQuery, MediaQueryList,
    ScriptingValue,
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
        let rules = StyleSheetParser::new(&mut input, &mut parser)
            .filter_map(Result::ok)
            .filter(rule_has_content)
            .collect();
        StyleSheet { rules, diagnostics }
    }
}

pub fn parse_media_queries(source: &str) -> MediaQueryList {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut diagnostics = CssDiagnostics::default();
    parse_media_query_list(&mut input, &mut diagnostics)
        .unwrap_or_else(|_| MediaQueryList::Any(vec![MediaQuery::Never]))
}

pub fn parse_declarations(source: &str) -> Vec<Declaration> {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    parse_declarations_from_parser(&mut input)
}
