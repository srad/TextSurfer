use cssparser::{Parser, ParserInput, Token, TokenSerializationType};

use super::{Lookup, MAX_COMPONENT_DEPTH};

pub(crate) fn is_custom_name(name: &str) -> bool {
    name.starts_with("--") && name != "--"
}

pub(crate) fn trim_css_whitespace(source: &str) -> &str {
    source.trim_matches([' ', '\t', '\n', '\r', '\u{c}'])
}

pub(crate) fn validate_declaration_value(source: &str, custom: bool) -> bool {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    validate_sequence(&mut parser, custom, 0).is_ok()
}

pub(crate) fn contains_var(source: &str) -> bool {
    let bytes = source.as_bytes();
    let quick_match = bytes.windows(4).any(|window| {
        window[0].eq_ignore_ascii_case(&b'v')
            && window[1].eq_ignore_ascii_case(&b'a')
            && window[2].eq_ignore_ascii_case(&b'r')
            && window[3] == b'('
    });
    if quick_match {
        return true;
    }
    if !bytes.contains(&b'\\') {
        return false;
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    contains_var_function(&mut parser, 0)
}

fn contains_var_function(input: &mut Parser<'_, '_>, depth: usize) -> bool {
    if depth > MAX_COMPONENT_DEPTH {
        return false;
    }
    while let Ok(token) = input.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => return true,
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                let mut found = false;
                let _ = input.parse_nested_block(|nested| {
                    found = contains_var_function(nested, depth + 1);
                    Ok::<_, cssparser::ParseError<'_, ()>>(())
                });
                if found {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub(super) fn dependencies(source: &str) -> Vec<String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut names = Vec::new();
    collect_dependencies(&mut parser, &mut names, 0);
    names
}

pub(super) fn substitute(
    source: &str,
    mut lookup: impl FnMut(&str) -> Lookup,
    limit: usize,
) -> Result<String, ()> {
    if source.len() > limit {
        return Err(());
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let result = substitute_sequence(&mut parser, source, &mut lookup, limit, 0)?;
    (result.len() <= limit).then_some(result).ok_or(())
}

fn validate_sequence<'i>(
    input: &mut Parser<'i, '_>,
    custom: bool,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    if depth > MAX_COMPONENT_DEPTH {
        return Err(input.new_custom_error(()));
    }
    while let Ok(token) = input.next_including_whitespace_and_comments().cloned() {
        if token.is_parse_error() || (custom && depth == 0 && matches!(token, Token::Delim('!'))) {
            return Err(input.new_custom_error(()));
        }
        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                input.parse_nested_block(|nested| validate_var(nested, custom, depth + 1))?;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                input.parse_nested_block(|nested| validate_sequence(nested, custom, depth + 1))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_var<'i>(
    input: &mut Parser<'i, '_>,
    custom: bool,
    depth: usize,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    let name = input.expect_ident_cloned()?;
    if !is_custom_name(&name) {
        return Err(input.new_custom_error(()));
    }
    if input.is_exhausted() {
        return Ok(());
    }
    input.expect_comma()?;
    validate_sequence(input, custom, depth)
}

fn collect_dependencies(input: &mut Parser<'_, '_>, names: &mut Vec<String>, depth: usize) {
    if depth > MAX_COMPONENT_DEPTH {
        return;
    }
    while let Ok(token) = input.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                let _ = input.parse_nested_block(|nested| {
                    if let Ok(name) = nested.expect_ident_cloned()
                        && is_custom_name(&name)
                    {
                        names.push(name.to_string());
                    }
                    if nested.expect_comma().is_ok() {
                        collect_dependencies(nested, names, depth + 1);
                    }
                    Ok::<_, cssparser::ParseError<'_, ()>>(())
                });
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                let _ = input.parse_nested_block(|nested| {
                    collect_dependencies(nested, names, depth + 1);
                    Ok::<_, cssparser::ParseError<'_, ()>>(())
                });
            }
            _ => {}
        }
    }
}

fn substitute_sequence<'i>(
    input: &mut Parser<'i, '_>,
    source: &'i str,
    lookup: &mut impl FnMut(&str) -> Lookup,
    limit: usize,
    depth: usize,
) -> Result<String, ()> {
    if depth > MAX_COMPONENT_DEPTH {
        return Err(());
    }
    let start = input.position().byte_index();
    let mut cursor = start;
    let mut output = String::new();
    while !input.is_exhausted() {
        let state = input.state();
        let token = input
            .next_including_whitespace_and_comments()
            .map_err(|_| ())?
            .clone();
        let token_start = state.position().byte_index();
        match token {
            Token::Function(name) if name.eq_ignore_ascii_case("var") => {
                push_bounded(&mut output, &source[cursor..token_start], limit)?;
                let replacement = input
                    .parse_nested_block(|nested| {
                        substitute_var(nested, source, lookup, limit, depth + 1)
                    })
                    .map_err(|_| ())?;
                let cursor_after = input.position().byte_index();
                if last_token_type(&output)
                    .needs_separator_when_before(first_token_type(&replacement))
                {
                    push_bounded(&mut output, "/**/", limit)?;
                }
                push_bounded(&mut output, &replacement, limit)?;
                if last_token_type(&replacement)
                    .needs_separator_when_before(first_token_type(&source[cursor_after..]))
                {
                    push_bounded(&mut output, "/**/", limit)?;
                }
                cursor = cursor_after;
            }
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => {
                push_bounded(
                    &mut output,
                    &source[cursor..input.position().byte_index()],
                    limit,
                )?;
                let mut inner_end = input.position().byte_index();
                let inner = input
                    .parse_nested_block(|nested| -> Result<String, cssparser::ParseError<'i, ()>> {
                        let value = substitute_sequence(nested, source, lookup, limit, depth + 1)
                            .map_err(|_| nested.new_custom_error(()))?;
                        inner_end = nested.position().byte_index();
                        Ok(value)
                    })
                    .map_err(|_| ())?;
                push_bounded(&mut output, &inner, limit)?;
                push_bounded(
                    &mut output,
                    &source[inner_end..input.position().byte_index()],
                    limit,
                )?;
                cursor = input.position().byte_index();
            }
            _ => {}
        }
    }
    push_bounded(
        &mut output,
        &source[cursor..input.position().byte_index()],
        limit,
    )?;
    Ok(output)
}

fn push_bounded(output: &mut String, value: &str, limit: usize) -> Result<(), ()> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|length| length > limit)
    {
        return Err(());
    }
    output.push_str(value);
    Ok(())
}

fn first_token_type(source: &str) -> TokenSerializationType {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    parser
        .next_including_whitespace_and_comments()
        .map_or(TokenSerializationType::Nothing, Token::serialization_type)
}

fn last_token_type(source: &str) -> TokenSerializationType {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut last = TokenSerializationType::Nothing;
    while let Ok(token) = parser.next_including_whitespace_and_comments() {
        last = token.serialization_type();
    }
    last
}

fn substitute_var<'i>(
    input: &mut Parser<'i, '_>,
    source: &'i str,
    lookup: &mut impl FnMut(&str) -> Lookup,
    limit: usize,
    depth: usize,
) -> Result<String, cssparser::ParseError<'i, ()>> {
    let name = input.expect_ident_cloned()?;
    let lookup_result = lookup(&name);
    let fallback = if input.is_exhausted() {
        None
    } else {
        input.expect_comma()?;
        Some(
            substitute_sequence(input, source, lookup, limit, depth)
                .map_err(|_| input.new_custom_error(()))?,
        )
    };
    match lookup_result {
        Lookup::Value(value) if value.len() <= limit => Ok(value.to_string()),
        Lookup::Value(_) => Err(input.new_custom_error(())),
        Lookup::Invalid | Lookup::Missing => fallback.ok_or_else(|| input.new_custom_error(())),
    }
}
