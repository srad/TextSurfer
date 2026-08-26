use cssparser::{Parser, ParserInput};

use super::tracks::custom_ident;
use crate::core::style::{GridLines, GridPlacement, GridStore};

/// Spans are `<integer [1,∞]>`; a line index of zero is invalid and would reach a panic inside
/// Taffy's `GridLine::into_origin_zero_line`, so it never leaves the cascade.
const MAX_LINE: i32 = i16::MAX as i32;

pub(super) fn parse_placement(source: &str, store: &mut GridStore) -> Option<GridPlacement> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_placement_parser(&mut parser, store)?.value;
    parser.expect_exhausted().ok()?;
    Some(value)
}

/// `grid-row` / `grid-column`: `<grid-line> [ / <grid-line> ]?`. An omitted end is `auto`, except
/// after a bare custom ident, which repeats on both sides.
pub(super) fn parse_lines(source: &str, store: &mut GridStore) -> Option<GridLines> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let start = parse_placement_parser(&mut parser, store)?;
    let end = if parser.is_exhausted() {
        implied_end(start)
    } else {
        parser.expect_delim('/').ok()?;
        parse_placement_parser(&mut parser, store)?.value
    };
    parser.expect_exhausted().ok()?;
    Some(GridLines {
        start: start.value,
        end,
    })
}

/// `grid-area`: one to four `<grid-line>`s. Each omitted value copies its opposite when that
/// opposite is a bare custom ident, and is `auto` otherwise.
pub(super) fn parse_area(source: &str, store: &mut GridStore) -> Option<(GridLines, GridLines)> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    loop {
        values.push(parse_placement_parser(&mut parser, store)?);
        if parser.is_exhausted() || values.len() == 4 {
            break;
        }
        parser.expect_delim('/').ok()?;
    }
    parser.expect_exhausted().ok()?;
    let row_start = values[0];
    let column_start = values.get(1).copied().unwrap_or(ParsedPlacement {
        value: implied_end(row_start),
        bare_name: row_start.bare_name,
    });
    let row_end = values
        .get(2)
        .map(|value| value.value)
        .unwrap_or_else(|| implied_end(row_start));
    let column_end = values
        .get(3)
        .map(|value| value.value)
        .unwrap_or_else(|| implied_end(column_start));
    Some((
        GridLines {
            start: row_start.value,
            end: row_end,
        },
        GridLines {
            start: column_start.value,
            end: column_end,
        },
    ))
}

fn implied_end(start: ParsedPlacement) -> GridPlacement {
    if start.bare_name {
        start.value
    } else {
        GridPlacement::Auto
    }
}

#[derive(Clone, Copy)]
struct ParsedPlacement {
    value: GridPlacement,
    bare_name: bool,
}

fn parse_placement_parser(
    parser: &mut Parser<'_, '_>,
    store: &mut GridStore,
) -> Option<ParsedPlacement> {
    if parser
        .try_parse(|input| input.expect_ident_matching("auto"))
        .is_ok()
    {
        return Some(ParsedPlacement {
            value: GridPlacement::Auto,
            bare_name: false,
        });
    }
    let mut span = false;
    let mut index: Option<i32> = None;
    let mut name = None;
    for _ in 0..3 {
        if !span
            && parser
                .try_parse(|input| input.expect_ident_matching("span"))
                .is_ok()
        {
            span = true;
            continue;
        }
        if index.is_none()
            && let Ok(value) = parser.try_parse(|input| input.expect_integer())
        {
            index = Some(value);
            continue;
        }
        if name.is_none()
            && let Ok(value) = parser.try_parse(|input| input.expect_ident_cloned())
        {
            name = Some(custom_ident(&value, store)?);
            continue;
        }
        break;
    }
    // `span` before a keyword is only a span if something followed it; a bare `span` is invalid.
    if span {
        let value = match (name, index) {
            (None, Some(count)) => (1..=MAX_LINE)
                .contains(&count)
                .then_some(GridPlacement::Span(count as u16)),
            (Some(name), None) => Some(GridPlacement::NamedSpan(name, 1)),
            (Some(name), Some(count)) => (1..=MAX_LINE)
                .contains(&count)
                .then_some(GridPlacement::NamedSpan(name, count as u16)),
            (None, None) => None,
        }?;
        return Some(ParsedPlacement {
            value,
            bare_name: false,
        });
    }
    let bare_name = name.is_some() && index.is_none();
    let value = match (name, index) {
        (None, Some(line)) => {
            (line != 0 && line.abs() <= MAX_LINE).then_some(GridPlacement::Line(line as i16))
        }
        (Some(name), None) => Some(GridPlacement::NamedLine(name, 1)),
        (Some(name), Some(line)) => (line != 0 && line.abs() <= MAX_LINE)
            .then_some(GridPlacement::NamedLine(name, line as i16)),
        (None, None) => None,
    }?;
    Some(ParsedPlacement { value, bare_name })
}
