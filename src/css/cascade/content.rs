use cssparser::{Parser, ParserInput, Token};

use crate::core::dom::{Document, Node, NodeId, attr_value};
use crate::core::style::ListStyleType;
use crate::css::values::{parse_ident, parse_list_style_type};

use super::counters::{CounterScopes, LIST_ITEM_COUNTER};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ContentSpec {
    /// `none`: no box at all, even where the UA supplies one.
    None,
    /// `normal`, and every CSS-wide keyword: defer to the UA. On `::marker` that is the default
    /// marker; on `::before`/`::after` the UA supplies nothing, so the pseudo-element is dropped.
    Normal,
    Pieces(Vec<ContentPiece>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ContentPiece {
    Text(String),
    Counter {
        name: String,
        style: ListStyleType,
    },
    Counters {
        name: String,
        separator: String,
        style: ListStyleType,
    },
    Attr(String),
}

/// Parses a `content` value. Anything the terminal cannot render (`url()`, quotes, images) makes
/// the whole declaration invalid, which is what the spec asks for and keeps half-rendered
/// generated content off the screen.
///
/// The CSS-wide keywords resolve to `Normal` rather than to "invalid": `content` is not inherited,
/// so `initial` and `unset` are its initial value `normal`, `revert` returns to the UA value the
/// caller passes as the fallback, and `inherit` is approximated the same way. Reporting them as
/// invalid would let an earlier declaration keep the side-table entry the later one meant to clear.
pub(super) fn parse_content(source: &str) -> Option<ContentSpec> {
    if let Some(keyword) = parse_ident(source) {
        return match keyword.as_str() {
            "none" => Some(ContentSpec::None),
            "normal" | "initial" | "inherit" | "unset" | "revert" => Some(ContentSpec::Normal),
            _ => None,
        };
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut pieces = Vec::new();
    while !parser.is_exhausted() {
        let token = parser.next().ok()?.clone();
        match token {
            Token::QuotedString(value) => pieces.push(ContentPiece::Text(value.to_string())),
            Token::Function(name) => {
                let name = name.to_ascii_lowercase();
                let piece = parser
                    .parse_nested_block(|input| parse_content_function(&name, input))
                    .ok()?;
                pieces.push(piece);
            }
            _ => return None,
        }
    }
    (!pieces.is_empty()).then_some(ContentSpec::Pieces(pieces))
}

fn parse_content_function<'i>(
    name: &str,
    input: &mut Parser<'i, '_>,
) -> Result<ContentPiece, cssparser::ParseError<'i, ()>> {
    let invalid = |input: &Parser<'i, '_>| input.new_custom_error(());
    match name {
        "attr" => {
            let attribute = input.expect_ident_cloned()?.to_string();
            input.expect_exhausted()?;
            Ok(ContentPiece::Attr(attribute))
        }
        "counter" => {
            let counter = input.expect_ident_cloned()?.to_string();
            let style = parse_counter_style_argument(input)?;
            input.expect_exhausted()?;
            Ok(ContentPiece::Counter {
                name: counter,
                style,
            })
        }
        "counters" => {
            let counter = input.expect_ident_cloned()?.to_string();
            input.expect_comma()?;
            let separator = input.expect_string_cloned()?.to_string();
            let style = parse_counter_style_argument(input)?;
            input.expect_exhausted()?;
            Ok(ContentPiece::Counters {
                name: counter,
                separator,
                style,
            })
        }
        _ => Err(invalid(input)),
    }
}

fn parse_counter_style_argument<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<ListStyleType, cssparser::ParseError<'i, ()>> {
    if input.try_parse(|input| input.expect_comma()).is_err() {
        return Ok(ListStyleType::Decimal);
    }
    let style = input.expect_ident_cloned()?;
    Ok(parse_list_style_type(&style).unwrap_or(ListStyleType::Decimal))
}

pub(super) fn resolve_content(
    pieces: &[ContentPiece],
    document: &Document,
    id: NodeId,
    counters: &CounterScopes,
) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            ContentPiece::Text(text) => out.push_str(text),
            ContentPiece::Counter { name, style } => {
                out.push_str(&style.render(counters.value(name)));
            }
            ContentPiece::Counters {
                name,
                separator,
                style,
            } => {
                let rendered: Vec<_> = counters
                    .values(name)
                    .into_iter()
                    .map(|value| style.render(value))
                    .collect();
                out.push_str(&rendered.join(separator));
            }
            ContentPiece::Attr(name) => {
                if let Some(Node::Element { attrs, .. }) = document.node(id)
                    && let Some(value) = attr_value(attrs, name)
                {
                    out.push_str(value);
                }
            }
        }
    }
    out
}

/// The UA marker carries its own trailing separator, so an author `::marker` that supplies its
/// own spacing is not charged for a second gutter.
pub(super) fn default_marker_text(
    list_style_type: ListStyleType,
    counters: &CounterScopes,
) -> Option<String> {
    if list_style_type == ListStyleType::None {
        return None;
    }
    let rendered = list_style_type.render(counters.value(LIST_ITEM_COUNTER));
    Some(if list_style_type.is_numeric() {
        format!("{rendered}. ")
    } else {
        format!("{rendered} ")
    })
}
