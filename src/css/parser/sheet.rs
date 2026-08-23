use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, Token,
};

use crate::css::selectors::{self, ParsedSelectors};

use super::ast::{CssRule, ImportRule, MediaRule, StyleRule, rule_has_content};
use super::declarations::parse_declarations_from_parser;
use super::diagnostics::{CssDiagnosticKind, CssDiagnostics};
use super::media::{MAX_MEDIA_NESTING, MediaQueryList, consume_all, parse_media_query_list};

pub(super) enum SheetAtRulePrelude {
    Media(MediaQueryList),
    Import(ImportRule),
    InvalidImport,
    Charset,
    Unsupported,
}

pub(super) struct SheetParser<'a> {
    media_depth: usize,
    imports_allowed: bool,
    diagnostics: &'a mut CssDiagnostics,
}

impl<'a> SheetParser<'a> {
    pub(super) fn new(diagnostics: &'a mut CssDiagnostics) -> Self {
        Self {
            media_depth: 0,
            imports_allowed: true,
            diagnostics,
        }
    }
}

fn parse_nested_rule_list<'i, 't>(
    input: &mut Parser<'i, 't>,
    media_depth: usize,
    diagnostics: &mut CssDiagnostics,
) -> Vec<CssRule> {
    let mut parser = SheetParser {
        media_depth,
        imports_allowed: false,
        diagnostics,
    };
    RuleBodyParser::new(input, &mut parser)
        .filter_map(Result::ok)
        .filter(rule_has_content)
        .collect()
}

impl<'i> QualifiedRuleParser<'i> for SheetParser<'_> {
    type Prelude = ParsedSelectors;
    type QualifiedRule = CssRule;
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        self.imports_allowed = false;
        let start = input.position();
        while input.next_including_whitespace_and_comments().is_ok() {}
        selectors::parse(input.slice_from(start).trim()).ok_or_else(|| input.new_custom_error(()))
    }

    fn parse_block<'t>(
        &mut self,
        selectors: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        Ok(CssRule::Style(StyleRule {
            selectors,
            declarations: parse_declarations_from_parser(input),
        }))
    }
}

impl<'i> DeclarationParser<'i> for SheetParser<'_> {
    type Declaration = CssRule;
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, CssRule, ()> for SheetParser<'_> {
    fn parse_declarations(&self) -> bool {
        false
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

impl<'i> AtRuleParser<'i> for SheetParser<'_> {
    type Prelude = SheetAtRulePrelude;
    type AtRule = CssRule;
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        if name.eq_ignore_ascii_case("media") {
            self.imports_allowed = false;
            return parse_media_query_list(input, self.diagnostics).map(SheetAtRulePrelude::Media);
        }
        if name.eq_ignore_ascii_case("import") {
            let location = input.current_source_location();
            if !self.imports_allowed {
                consume_all(input)?;
                self.diagnostics
                    .record(CssDiagnosticKind::InvalidImportForm, location);
                return Ok(SheetAtRulePrelude::InvalidImport);
            }
            let url = match input.expect_url_or_string() {
                Ok(url) => url.to_string(),
                Err(error) => {
                    self.diagnostics
                        .record(CssDiagnosticKind::InvalidImportForm, location);
                    return Err(error.into());
                }
            };
            if contains_unsupported_import_option(input) {
                consume_all(input)?;
                self.diagnostics
                    .record(CssDiagnosticKind::InvalidImportForm, location);
                return Ok(SheetAtRulePrelude::InvalidImport);
            }
            let queries = parse_media_query_list(input, self.diagnostics)?;
            Ok(SheetAtRulePrelude::Import(ImportRule { url, queries }))
        } else if name.eq_ignore_ascii_case("charset") {
            consume_all(input)?;
            Ok(SheetAtRulePrelude::Charset)
        } else {
            self.imports_allowed = false;
            consume_all(input)?;
            Ok(SheetAtRulePrelude::Unsupported)
        }
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        start: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        match prelude {
            SheetAtRulePrelude::Media(_) => self.diagnostics.record(
                CssDiagnosticKind::InvalidMediaRuleForm,
                start.source_location(),
            ),
            SheetAtRulePrelude::Import(rule) => return Ok(CssRule::Import(rule)),
            SheetAtRulePrelude::InvalidImport
            | SheetAtRulePrelude::Charset
            | SheetAtRulePrelude::Unsupported => {}
        }
        Err(())
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::AtRule, ParseError<'i, Self::Error>> {
        match prelude {
            SheetAtRulePrelude::Media(queries) => {
                if self.media_depth >= MAX_MEDIA_NESTING {
                    self.diagnostics.record(
                        CssDiagnosticKind::NestingLimitExceeded,
                        start.source_location(),
                    );
                    consume_all(input)?;
                    return Ok(CssRule::Media(MediaRule {
                        queries,
                        rules: Vec::new(),
                    }));
                }
                let rules = parse_nested_rule_list(input, self.media_depth + 1, self.diagnostics);
                Ok(CssRule::Media(MediaRule { queries, rules }))
            }
            SheetAtRulePrelude::Import(_) => {
                self.diagnostics.record(
                    CssDiagnosticKind::InvalidImportForm,
                    start.source_location(),
                );
                consume_all(input)?;
                Err(input.new_custom_error(()))
            }
            SheetAtRulePrelude::InvalidImport => {
                consume_all(input)?;
                Err(input.new_custom_error(()))
            }
            SheetAtRulePrelude::Charset | SheetAtRulePrelude::Unsupported => {
                consume_all(input)?;
                Err(input.new_custom_error(()))
            }
        }
    }
}

fn contains_unsupported_import_option<'i, 't>(input: &mut Parser<'i, 't>) -> bool {
    let start = input.state();
    let mut found = false;
    while let Ok(token) = input.next_including_whitespace_and_comments() {
        let nested = matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        );
        if matches!(
            token,
            Token::Ident(name) | Token::Function(name)
                if name.eq_ignore_ascii_case("layer") || name.eq_ignore_ascii_case("supports")
        ) {
            found = true;
            break;
        }
        if nested
            && input
                .parse_nested_block(|nested| consume_all(nested))
                .is_err()
        {
            break;
        }
    }
    input.reset(&start);
    found
}
