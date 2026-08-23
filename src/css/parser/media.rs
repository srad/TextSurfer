use cssparser::{ParseError, Parser, ParserInput, Token};

use super::diagnostics::{CssDiagnosticKind, CssDiagnostics};

pub(super) const MAX_MEDIA_NESTING: usize = 64;

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

pub fn parse_media_queries(source: &str) -> MediaQueryList {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut diagnostics = CssDiagnostics::default();
    parse_media_query_list(&mut input, &mut diagnostics)
        .unwrap_or_else(|_| MediaQueryList::Any(vec![MediaQuery::Never]))
}

pub(super) fn consume_all<'i, 't>(input: &mut Parser<'i, 't>) -> Result<(), ParseError<'i, ()>> {
    while input.next_including_whitespace_and_comments().is_ok() {}
    Ok(())
}

pub(super) fn parse_media_query_list<'i, 't>(
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
