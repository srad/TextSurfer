use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation, StyleSheetParser,
    Token, parse_important,
};

use crate::css::selectors::{self, ParsedSelectors};

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
}

#[derive(Clone, Debug)]
pub struct MediaRule {
    pub queries: MediaQueryList,
    pub rules: Vec<CssRule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaQueryList {
    Always,
    Any(Vec<MediaQuery>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaQuery {
    Type { negated: bool, media_type: String },
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssSourcePosition {
    pub line: u32,
    pub column: u32,
}

impl From<SourceLocation> for CssSourcePosition {
    fn from(location: SourceLocation) -> Self {
        Self {
            line: location.line,
            column: location.column,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssDiagnosticKind {
    IgnoredImport,
    InvalidImportForm,
    InvalidMediaRuleForm,
    UnsupportedMediaQuery,
    NestingLimitExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssDiagnostic {
    pub kind: CssDiagnosticKind,
    pub position: CssSourcePosition,
}

#[derive(Clone, Debug, Default)]
pub struct CssDiagnostics {
    total: usize,
    entries: Vec<CssDiagnostic>,
}

impl CssDiagnostics {
    pub fn total(&self) -> usize {
        self.total
    }

    pub fn entries(&self) -> &[CssDiagnostic] {
        &self.entries
    }

    pub fn truncated(&self) -> bool {
        self.total > self.entries.len()
    }

    fn record(&mut self, kind: CssDiagnosticKind, location: SourceLocation) {
        self.total = self.total.saturating_add(1);
        if self.entries.len() < MAX_RETAINED_DIAGNOSTICS {
            self.entries.push(CssDiagnostic {
                kind,
                position: location.into(),
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    pub rules: Vec<CssRule>,
    pub diagnostics: CssDiagnostics,
}

const MAX_MEDIA_NESTING: usize = 64;
const MAX_RETAINED_DIAGNOSTICS: usize = 64;

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
        let mut parser = SheetParser {
            media_depth: 0,
            diagnostics: &mut diagnostics,
        };
        let rules = StyleSheetParser::new(&mut input, &mut parser)
            .filter_map(Result::ok)
            .filter(rule_has_content)
            .collect();
        StyleSheet { rules, diagnostics }
    }
}

pub fn parse_declarations(source: &str) -> Vec<Declaration> {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    parse_declarations_from_parser(&mut input)
}

fn parse_declarations_from_parser<'i, 't>(input: &mut Parser<'i, 't>) -> Vec<Declaration> {
    let mut parser = DeclarationListParser;
    RuleBodyParser::new(input, &mut parser)
        .filter_map(Result::ok)
        .collect()
}

struct DeclarationListParser;

impl<'i> DeclarationParser<'i> for DeclarationListParser {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _declaration_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let value_start = input.position();
        let mut important = false;
        let value_end = loop {
            let state = input.state();
            match input.next_including_whitespace_and_comments() {
                Ok(Token::Delim('!')) => {
                    input.reset(&state);
                    if input.try_parse(parse_important).is_ok() && input.is_exhausted() {
                        important = true;
                        break state.position();
                    }
                    input.reset(&state);
                    input.next_including_whitespace_and_comments()?;
                }
                Ok(token) => {
                    if matches!(
                        token,
                        Token::Function(_)
                            | Token::ParenthesisBlock
                            | Token::SquareBracketBlock
                            | Token::CurlyBracketBlock
                    ) {
                        input.parse_nested_block(|nested| {
                            while nested.next_including_whitespace_and_comments().is_ok() {}
                            Ok::<_, ParseError<'i, ()>>(())
                        })?;
                    }
                }
                Err(_) => break input.position(),
            }
        };
        Ok(Declaration {
            name: name.to_ascii_lowercase(),
            value: input.slice(value_start..value_end).trim().to_string(),
            important,
        })
    }
}

impl<'i> AtRuleParser<'i> for DeclarationListParser {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = ();
}

impl<'i> QualifiedRuleParser<'i> for DeclarationListParser {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, Declaration, ()> for DeclarationListParser {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

fn rule_has_content(rule: &CssRule) -> bool {
    match rule {
        CssRule::Style(rule) => !rule.declarations.is_empty(),
        CssRule::Media(_) => true,
    }
}

fn consume_all<'i, 't>(input: &mut Parser<'i, 't>) -> Result<(), ParseError<'i, ()>> {
    while input.next_including_whitespace_and_comments().is_ok() {}
    Ok(())
}

fn parse_media_query_list<'i, 't>(
    input: &mut Parser<'i, 't>,
    diagnostics: &mut CssDiagnostics,
) -> Result<MediaQueryList, ParseError<'i, ()>> {
    if input.try_parse(Parser::expect_exhausted).is_ok() {
        return Ok(MediaQueryList::Always);
    }
    let queries = input.parse_comma_separated(|member| {
        let location = member.current_source_location();
        match member.try_parse(parse_type_media_query) {
            Ok(query) => Ok(query),
            Err(_) => {
                consume_all(member)?;
                diagnostics.record(CssDiagnosticKind::UnsupportedMediaQuery, location);
                Ok(MediaQuery::Never)
            }
        }
    })?;
    Ok(MediaQueryList::Any(queries))
}

fn parse_type_media_query<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<MediaQuery, ParseError<'i, ()>> {
    let first = input.expect_ident_cloned()?.to_ascii_lowercase();
    let (negated, media_type) = if first == "not" || first == "only" {
        (
            first == "not",
            input.expect_ident_cloned()?.to_ascii_lowercase(),
        )
    } else {
        (false, first)
    };
    input.expect_exhausted()?;
    if ["not", "only", "and", "or", "layer"].contains(&media_type.as_str()) {
        return Err(input.new_custom_error(()));
    }
    Ok(MediaQuery::Type {
        negated,
        media_type,
    })
}

enum SheetAtRulePrelude {
    Media(MediaQueryList),
    Import,
    Unsupported,
}

struct SheetParser<'a> {
    media_depth: usize,
    diagnostics: &'a mut CssDiagnostics,
}

fn parse_nested_rule_list<'i, 't>(
    input: &mut Parser<'i, 't>,
    media_depth: usize,
    diagnostics: &mut CssDiagnostics,
) -> Vec<CssRule> {
    let mut parser = SheetParser {
        media_depth,
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
            return parse_media_query_list(input, self.diagnostics).map(SheetAtRulePrelude::Media);
        }
        consume_all(input)?;
        if name.eq_ignore_ascii_case("import") {
            Ok(SheetAtRulePrelude::Import)
        } else {
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
            SheetAtRulePrelude::Import => self
                .diagnostics
                .record(CssDiagnosticKind::IgnoredImport, start.source_location()),
            SheetAtRulePrelude::Unsupported => {}
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
            SheetAtRulePrelude::Import => {
                self.diagnostics.record(
                    CssDiagnosticKind::InvalidImportForm,
                    start.source_location(),
                );
                consume_all(input)?;
                Err(input.new_custom_error(()))
            }
            SheetAtRulePrelude::Unsupported => {
                consume_all(input)?;
                Err(input.new_custom_error(()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selector_rules_and_declarations() {
        let sheet = CssparserParser
            .parse("article > p.note { display: block; margin: 1px 2px; } invalid { color }");
        assert_eq!(sheet.rules.len(), 1);
        let CssRule::Style(rule) = &sheet.rules[0] else {
            panic!("expected a style rule");
        };
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].name, "display");
    }

    fn css_parser_contract(parser: &dyn CssParser) {
        let sheet = parser.parse(
            "@media { a { display: block } }
             @media only SCREEN, not print, unknown, not unknown { b { display: block } }
             @media (width: 1px), screen and (color), screen { c { display: block } }
             @import url(theme.css);
             d { display: block }",
        );
        assert_eq!(sheet.rules.len(), 4);
        let CssRule::Media(empty) = &sheet.rules[0] else {
            panic!("expected empty media rule");
        };
        assert_eq!(empty.queries, MediaQueryList::Always);
        let CssRule::Media(types) = &sheet.rules[1] else {
            panic!("expected type media rule");
        };
        assert_eq!(
            types.queries,
            MediaQueryList::Any(vec![
                MediaQuery::Type {
                    negated: false,
                    media_type: "screen".to_string(),
                },
                MediaQuery::Type {
                    negated: true,
                    media_type: "print".to_string(),
                },
                MediaQuery::Type {
                    negated: false,
                    media_type: "unknown".to_string(),
                },
                MediaQuery::Type {
                    negated: true,
                    media_type: "unknown".to_string(),
                },
            ])
        );
        let CssRule::Media(partial) = &sheet.rules[2] else {
            panic!("expected partially unsupported media rule");
        };
        assert_eq!(
            partial.queries,
            MediaQueryList::Any(vec![
                MediaQuery::Never,
                MediaQuery::Never,
                MediaQuery::Type {
                    negated: false,
                    media_type: "screen".to_string(),
                },
            ])
        );
        assert!(matches!(sheet.rules[3], CssRule::Style(_)));
        assert_eq!(sheet.diagnostics.total(), 3);
        assert_eq!(
            sheet
                .diagnostics
                .entries()
                .iter()
                .map(|diagnostic| diagnostic.kind)
                .collect::<Vec<_>>(),
            vec![
                CssDiagnosticKind::UnsupportedMediaQuery,
                CssDiagnosticKind::UnsupportedMediaQuery,
                CssDiagnosticKind::IgnoredImport,
            ]
        );
    }

    #[test]
    fn cssparser_parser_passes_the_css_parser_contract() {
        css_parser_contract(&CssparserParser);
    }

    #[test]
    fn invalid_at_rule_forms_recover_to_following_rules() {
        let sheet = CssparserParser.parse(
            "@media screen; p { display: block }
             @import url(theme.css) { ignored { display: none } }
             @unknown test { ignored { display: none } }
             div { display: block }",
        );
        assert_eq!(sheet.rules.len(), 2);
        assert!(
            sheet
                .diagnostics
                .entries()
                .iter()
                .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidMediaRuleForm)
        );
        assert!(
            sheet
                .diagnostics
                .entries()
                .iter()
                .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidImportForm)
        );
        assert_eq!(sheet.diagnostics.total(), 2);
    }

    #[test]
    fn invalid_media_members_never_become_an_active_empty_list() {
        let sheet = CssparserParser.parse(
            "@media , { a { display: none } }
             @media scr\\65 en { b { display: block } }
             @media not print and (width) { c { display: none } }",
        );
        let CssRule::Media(invalid) = &sheet.rules[0] else {
            panic!("expected invalid media rule");
        };
        assert_eq!(
            invalid.queries,
            MediaQueryList::Any(vec![MediaQuery::Never, MediaQuery::Never])
        );
        let CssRule::Media(escaped) = &sheet.rules[1] else {
            panic!("expected escaped media rule");
        };
        assert_eq!(
            escaped.queries,
            MediaQueryList::Any(vec![MediaQuery::Type {
                negated: false,
                media_type: "screen".to_string(),
            }])
        );
        let CssRule::Media(unsupported_not) = &sheet.rules[2] else {
            panic!("expected unsupported media rule");
        };
        assert_eq!(
            unsupported_not.queries,
            MediaQueryList::Any(vec![MediaQuery::Never])
        );
    }

    #[test]
    fn media_nesting_and_diagnostics_are_bounded() {
        let accepted = format!(
            "{}p {{ display: block }}{}",
            "@media screen {".repeat(MAX_MEDIA_NESTING),
            "}".repeat(MAX_MEDIA_NESTING)
        );
        assert_eq!(CssparserParser.parse(&accepted).diagnostics.total(), 0);

        let rejected = format!(
            "{}p {{ display: block }}{} q {{ display: block }}",
            "@media screen {".repeat(MAX_MEDIA_NESTING + 1),
            "}".repeat(MAX_MEDIA_NESTING + 1)
        );
        let rejected = CssparserParser.parse(&rejected);
        assert!(
            rejected
                .diagnostics
                .entries()
                .iter()
                .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::NestingLimitExceeded)
        );

        let imports = "@import url(theme.css);".repeat(MAX_RETAINED_DIAGNOSTICS + 3);
        let diagnostics = CssparserParser.parse(&imports).diagnostics;
        assert_eq!(diagnostics.total(), MAX_RETAINED_DIAGNOSTICS + 3);
        assert_eq!(diagnostics.entries().len(), MAX_RETAINED_DIAGNOSTICS);
        assert!(diagnostics.truncated());
    }

    #[test]
    fn inline_declarations_preserve_values_and_important() {
        assert_eq!(
            parse_declarations("display: none !important; white-space: pre"),
            vec![
                Declaration {
                    name: "display".to_string(),
                    value: "none".to_string(),
                    important: true,
                },
                Declaration {
                    name: "white-space".to_string(),
                    value: "pre".to_string(),
                    important: false,
                },
            ]
        );
    }

    #[test]
    fn important_uses_css_token_rules_instead_of_string_suffixes() {
        let declarations = parse_declarations(
            "display: none ! /**/ IMPORTANT; color: var(--looks-like-!important)",
        );
        assert!(declarations[0].important);
        assert_eq!(declarations[0].value, "none");
        assert!(!declarations[1].important);
        assert_eq!(declarations[1].value, "var(--looks-like-!important)");
    }
}
