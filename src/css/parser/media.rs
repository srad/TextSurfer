use cssparser::{ParseError, Parser, ParserInput, Token};

use crate::core::style::CssLength;
use crate::css::values::parse_signed_length_token;

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
    Greater,
    Less,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaBound {
    pub value: CssLength,
    pub inclusive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionCondition {
    Boolean,
    Compare {
        comparison: MediaComparison,
        value: CssLength,
    },
    Between {
        lower: MediaBound,
        upper: MediaBound,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaFeature {
    Scripting(Option<ScriptingValue>),
    PrefersColorScheme(Option<ColorScheme>),
    Dimension {
        axis: MediaAxis,
        condition: DimensionCondition,
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
    if let Ok(name) = input.try_parse(Parser::expect_ident_cloned) {
        let name = name.to_ascii_lowercase();
        if input.is_exhausted() {
            return match name.as_str() {
                "scripting" => Ok(MediaFeature::Scripting(None)),
                "prefers-color-scheme" => Ok(MediaFeature::PrefersColorScheme(None)),
                "width" | "height" => Ok(MediaFeature::Dimension {
                    axis: range_media_axis(&name).expect("dimension name checked"),
                    condition: DimensionCondition::Boolean,
                }),
                _ => Err(input.new_custom_error(())),
            };
        }
        if input.try_parse(Parser::expect_colon).is_ok() {
            return parse_colon_media_feature(input, &name);
        }
        let axis = range_media_axis(&name).ok_or_else(|| input.new_custom_error(()))?;
        let comparison = parse_media_comparison(input)?;
        let value = parse_media_length(input)?;
        input.expect_exhausted()?;
        return Ok(MediaFeature::Dimension {
            axis,
            condition: DimensionCondition::Compare { comparison, value },
        });
    }

    let first_value = parse_media_length(input)?;
    let first_comparison = parse_media_comparison(input)?;
    let name = input.expect_ident_cloned()?.to_ascii_lowercase();
    let axis = range_media_axis(&name).ok_or_else(|| input.new_custom_error(()))?;
    if input.is_exhausted() {
        return Ok(MediaFeature::Dimension {
            axis,
            condition: DimensionCondition::Compare {
                comparison: invert_media_comparison(first_comparison),
                value: first_value,
            },
        });
    }
    let second_comparison = parse_media_comparison(input)?;
    let second_value = parse_media_length(input)?;
    input.expect_exhausted()?;
    let (lower, upper) = media_range(
        first_value,
        first_comparison,
        second_comparison,
        second_value,
    )
    .ok_or_else(|| input.new_custom_error(()))?;
    Ok(MediaFeature::Dimension {
        axis,
        condition: DimensionCondition::Between { lower, upper },
    })
}

fn parse_colon_media_feature<'i, 't>(
    input: &mut Parser<'i, 't>,
    name: &str,
) -> Result<MediaFeature, ParseError<'i, ()>> {
    let feature = match name {
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
            let axis = legacy_media_axis(name).expect("dimension name checked");
            let comparison = if name.starts_with("min-") {
                MediaComparison::Minimum
            } else if name.starts_with("max-") {
                MediaComparison::Maximum
            } else {
                MediaComparison::Equal
            };
            MediaFeature::Dimension {
                axis,
                condition: DimensionCondition::Compare {
                    comparison,
                    value: parse_media_length(input)?,
                },
            }
        }
        _ => return Err(input.new_custom_error(())),
    };
    input.expect_exhausted()?;
    Ok(feature)
}

fn legacy_media_axis(name: &str) -> Option<MediaAxis> {
    if name.ends_with("width") {
        Some(MediaAxis::Width)
    } else if name.ends_with("height") {
        Some(MediaAxis::Height)
    } else {
        None
    }
}

fn range_media_axis(name: &str) -> Option<MediaAxis> {
    match name {
        "width" => Some(MediaAxis::Width),
        "height" => Some(MediaAxis::Height),
        _ => None,
    }
}

fn parse_media_length<'i, 't>(input: &mut Parser<'i, 't>) -> Result<CssLength, ParseError<'i, ()>> {
    parse_signed_length_token(input).ok_or_else(|| input.new_custom_error(()))
}

fn parse_media_comparison<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<MediaComparison, ParseError<'i, ()>> {
    match input.next()? {
        Token::Delim('=') => Ok(MediaComparison::Equal),
        Token::Delim('<') => Ok(
            if input.try_parse(|input| input.expect_delim('=')).is_ok() {
                MediaComparison::Maximum
            } else {
                MediaComparison::Less
            },
        ),
        Token::Delim('>') => Ok(
            if input.try_parse(|input| input.expect_delim('=')).is_ok() {
                MediaComparison::Minimum
            } else {
                MediaComparison::Greater
            },
        ),
        _ => Err(input.new_custom_error(())),
    }
}

fn invert_media_comparison(comparison: MediaComparison) -> MediaComparison {
    match comparison {
        MediaComparison::Equal => MediaComparison::Equal,
        MediaComparison::Minimum => MediaComparison::Maximum,
        MediaComparison::Maximum => MediaComparison::Minimum,
        MediaComparison::Greater => MediaComparison::Less,
        MediaComparison::Less => MediaComparison::Greater,
    }
}

fn media_range(
    first_value: CssLength,
    first_comparison: MediaComparison,
    second_comparison: MediaComparison,
    second_value: CssLength,
) -> Option<(MediaBound, MediaBound)> {
    let bound = |value, comparison| MediaBound {
        value,
        inclusive: matches!(
            comparison,
            MediaComparison::Minimum | MediaComparison::Maximum
        ),
    };
    match (first_comparison, second_comparison) {
        (
            first @ (MediaComparison::Less | MediaComparison::Maximum),
            second @ (MediaComparison::Less | MediaComparison::Maximum),
        ) => Some((bound(first_value, first), bound(second_value, second))),
        (
            first @ (MediaComparison::Greater | MediaComparison::Minimum),
            second @ (MediaComparison::Greater | MediaComparison::Minimum),
        ) => Some((bound(second_value, second), bound(first_value, first))),
        _ => None,
    }
}
