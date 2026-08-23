use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, Token, parse_important,
};

use super::ast::Declaration;

pub(super) fn parse_declarations_from_parser<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Vec<Declaration> {
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
