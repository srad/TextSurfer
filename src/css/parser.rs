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
    Import(ImportRule),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRule {
    pub url: String,
    pub queries: MediaQueryList,
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
    Type {
        negated: bool,
        media_type: String,
    },
    Condition {
        negated: bool,
        media_type: Option<String>,
        features: Vec<MediaFeature>,
    },
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptingValue {
    None,
    InitialOnly,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAxis {
    Width,
    Height,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaComparison {
    Equal,
    Minimum,
    Maximum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaFeature {
    Scripting(Option<ScriptingValue>),
    PrefersColorScheme(Option<ColorScheme>),
    Dimension {
        axis: MediaAxis,
        comparison: MediaComparison,
        value: Option<u16>,
    },
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

pub fn parse_media_queries(source: &str) -> MediaQueryList {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut diagnostics = CssDiagnostics::default();
    parse_media_query_list(&mut input, &mut diagnostics)
        .unwrap_or_else(|_| MediaQueryList::Any(vec![MediaQuery::Never]))
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
            imports_allowed: true,
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
        CssRule::Import(_) => true,
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
        match member.try_parse(parse_media_query) {
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

fn parse_media_query<'i, 't>(input: &mut Parser<'i, 't>) -> Result<MediaQuery, ParseError<'i, ()>> {
    let modifier_start = input.state();
    let modifier = input
        .try_parse(Parser::expect_ident_cloned)
        .ok()
        .map(|ident| ident.to_ascii_lowercase())
        .filter(|ident| ident == "not" || ident == "only");
    if modifier.is_none() {
        input.reset(&modifier_start);
    }
    let negated = modifier.as_deref() == Some("not");
    let media_type = input
        .try_parse(Parser::expect_ident_cloned)
        .ok()
        .map(|ident| ident.to_ascii_lowercase());
    if modifier.as_deref() == Some("only") && media_type.is_none() {
        return Err(input.new_custom_error(()));
    }
    if media_type
        .as_deref()
        .is_some_and(|name| ["not", "only", "and", "or", "layer"].contains(&name))
    {
        return Err(input.new_custom_error(()));
    }
    let mut features = Vec::new();
    if media_type.is_none() {
        features.push(parse_media_feature_block(input)?);
    }
    while !input.is_exhausted() {
        if media_type.is_some() || !features.is_empty() {
            input.expect_ident_matching("and")?;
        }
        features.push(parse_media_feature_block(input)?);
    }
    if features.is_empty() {
        Ok(MediaQuery::Type {
            negated,
            media_type: media_type.expect("media type checked"),
        })
    } else {
        Ok(MediaQuery::Condition {
            negated,
            media_type,
            features,
        })
    }
}

fn parse_media_feature_block<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<MediaFeature, ParseError<'i, ()>> {
    match input.next()? {
        Token::ParenthesisBlock => input.parse_nested_block(parse_media_feature),
        _ => Err(input.new_custom_error(())),
    }
}

fn parse_media_feature<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<MediaFeature, ParseError<'i, ()>> {
    let name = input.expect_ident_cloned()?.to_ascii_lowercase();
    if input.is_exhausted() {
        return match name.as_str() {
            "scripting" => Ok(MediaFeature::Scripting(None)),
            "prefers-color-scheme" => Ok(MediaFeature::PrefersColorScheme(None)),
            "width" => Ok(MediaFeature::Dimension {
                axis: MediaAxis::Width,
                comparison: MediaComparison::Equal,
                value: None,
            }),
            "height" => Ok(MediaFeature::Dimension {
                axis: MediaAxis::Height,
                comparison: MediaComparison::Equal,
                value: None,
            }),
            _ => Err(input.new_custom_error(())),
        };
    }
    input.expect_colon()?;
    let feature = match name.as_str() {
        "scripting" => {
            let value = match input.expect_ident_cloned()?.to_ascii_lowercase().as_str() {
                "none" => ScriptingValue::None,
                "initial-only" => ScriptingValue::InitialOnly,
                "enabled" => ScriptingValue::Enabled,
                _ => return Err(input.new_custom_error(())),
            };
            MediaFeature::Scripting(Some(value))
        }
        "prefers-color-scheme" => {
            let value = match input.expect_ident_cloned()?.to_ascii_lowercase().as_str() {
                "light" => ColorScheme::Light,
                "dark" => ColorScheme::Dark,
                _ => return Err(input.new_custom_error(())),
            };
            MediaFeature::PrefersColorScheme(Some(value))
        }
        "width" | "min-width" | "max-width" | "height" | "min-height" | "max-height" => {
            let axis = if name.ends_with("width") {
                MediaAxis::Width
            } else {
                MediaAxis::Height
            };
            let comparison = if name.starts_with("min-") {
                MediaComparison::Minimum
            } else if name.starts_with("max-") {
                MediaComparison::Maximum
            } else {
                MediaComparison::Equal
            };
            MediaFeature::Dimension {
                axis,
                comparison,
                value: Some(parse_media_length(input)?),
            }
        }
        _ => return Err(input.new_custom_error(())),
    };
    input.expect_exhausted()?;
    Ok(feature)
}

fn parse_media_length<'i, 't>(input: &mut Parser<'i, 't>) -> Result<u16, ParseError<'i, ()>> {
    let value = match input.next()? {
        Token::Number { value, .. } if *value == 0.0 => 0.0,
        Token::Dimension { value, unit, .. }
            if value.is_finite()
                && *value >= 0.0
                && (unit.eq_ignore_ascii_case("px") || unit.eq_ignore_ascii_case("ch")) =>
        {
            *value
        }
        _ => return Err(input.new_custom_error(())),
    };
    Ok(value.round().clamp(0.0, u16::MAX as f32) as u16)
}

enum SheetAtRulePrelude {
    Media(MediaQueryList),
    Import(ImportRule),
    InvalidImport,
    Charset,
    Unsupported,
}

struct SheetParser<'a> {
    media_depth: usize,
    imports_allowed: bool,
    diagnostics: &'a mut CssDiagnostics,
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
                MediaQuery::Condition {
                    negated: false,
                    media_type: None,
                    features: vec![MediaFeature::Dimension {
                        axis: MediaAxis::Width,
                        comparison: MediaComparison::Equal,
                        value: Some(1),
                    }],
                },
                MediaQuery::Never,
                MediaQuery::Type {
                    negated: false,
                    media_type: "screen".to_string(),
                },
            ])
        );
        assert!(matches!(sheet.rules[3], CssRule::Style(_)));
        assert_eq!(sheet.diagnostics.total(), 2);
        assert_eq!(
            sheet
                .diagnostics
                .entries()
                .iter()
                .map(|diagnostic| diagnostic.kind)
                .collect::<Vec<_>>(),
            vec![
                CssDiagnosticKind::UnsupportedMediaQuery,
                CssDiagnosticKind::InvalidImportForm,
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
        let CssRule::Media(supported_not) = &sheet.rules[2] else {
            panic!("expected supported media rule");
        };
        assert_eq!(
            supported_not.queries,
            MediaQueryList::Any(vec![MediaQuery::Condition {
                negated: true,
                media_type: Some("print".to_string()),
                features: vec![MediaFeature::Dimension {
                    axis: MediaAxis::Width,
                    comparison: MediaComparison::Equal,
                    value: None,
                }],
            }])
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

        let imports = format!(
            "p {{ display: block }}{}",
            "@import url(theme.css);".repeat(MAX_RETAINED_DIAGNOSTICS + 3)
        );
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
    fn leading_imports_are_retained_and_late_nested_and_qualified_forms_are_rejected() {
        let sheet = CssparserParser.parse(
            "@charset \"utf-8\";
             @import \"a.css\" screen and (min-width: 20px);
             @import url(b.css) layer(theme);
             p { display: block }
             @import 'late.css';
             @media screen { @import 'nested.css'; span { display: block } }",
        );
        let CssRule::Import(rule) = &sheet.rules[0] else {
            panic!("expected retained import");
        };
        assert_eq!(rule.url, "a.css");
        assert!(matches!(
            rule.queries,
            MediaQueryList::Any(ref queries) if matches!(queries[0], MediaQuery::Condition { .. })
        ));
        for invalid in [
            "@import url(b.css) layer(theme);",
            "p { display:block } @import 'late.css';",
            "@media screen { @import 'nested.css'; span { display:block } }",
        ] {
            assert!(
                CssparserParser
                    .parse(invalid)
                    .diagnostics
                    .entries()
                    .iter()
                    .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidImportForm),
                "{invalid}"
            );
        }
    }

    #[test]
    fn media_features_parse_with_ranges_and_fail_closed_for_unsupported_syntax() {
        let queries = parse_media_queries(
            "not (scripting: enabled), only screen and (prefers-color-scheme: dark) and \
             (min-width: 79.6px) and (max-height: 30ch), (width > 10px)",
        );
        let MediaQueryList::Any(queries) = queries else {
            panic!("expected media members");
        };
        assert!(matches!(
            queries[0],
            MediaQuery::Condition { negated: true, .. }
        ));
        assert!(matches!(
            queries[1],
            MediaQuery::Condition { negated: false, .. }
        ));
        assert_eq!(queries[2], MediaQuery::Never);
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
