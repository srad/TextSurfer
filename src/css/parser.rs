use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
    parse_important,
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

#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

pub trait CssParser: Send + Sync {
    fn parse(&self, source: &str) -> StyleSheet;
}

#[derive(Default)]
pub struct CssparserParser;

impl CssParser for CssparserParser {
    fn parse(&self, source: &str) -> StyleSheet {
        let mut input = ParserInput::new(source);
        let mut input = Parser::new(&mut input);
        let mut parser = SheetParser;
        let rules = StyleSheetParser::new(&mut input, &mut parser)
            .filter_map(Result::ok)
            .filter(|rule| !rule.declarations.is_empty())
            .collect();
        StyleSheet { rules }
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

struct SheetParser;

impl<'i> QualifiedRuleParser<'i> for SheetParser {
    type Prelude = ParsedSelectors;
    type QualifiedRule = StyleRule;
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
        Ok(StyleRule {
            selectors,
            declarations: parse_declarations_from_parser(input),
        })
    }
}

impl<'i> AtRuleParser<'i> for SheetParser {
    type Prelude = ();
    type AtRule = StyleRule;
    type Error = ();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selector_rules_and_declarations() {
        let sheet = CssparserParser
            .parse("article > p.note { display: block; margin: 1px 2px; } invalid { color }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations.len(), 2);
        assert_eq!(sheet.rules[0].declarations[0].name, "display");
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
