use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use crate::core::style::{
    AxisCellLength, ComputedStyle, CssNumber, FlexBasis, FlexDirection, FlexStyle, FlexWrap,
    LengthAxis,
};
use crate::css::values::{parse_ident, parse_length_token, percentage_value};

pub(super) fn apply_flex_declaration(
    style: &mut ComputedStyle,
    property: &str,
    source: &str,
    media: MediaContext,
) -> bool {
    match property {
        "flex-direction" => assign(&mut style.flex.direction, parse_direction(source)),
        "flex-wrap" => assign(&mut style.flex.wrap, parse_wrap(source)),
        "flex-flow" => assign_flow(&mut style.flex, source),
        "flex-grow" => assign(&mut style.flex.grow, parse_number(source)),
        "flex-shrink" => assign(&mut style.flex.shrink, parse_number(source)),
        "flex-basis" => assign(&mut style.flex.basis, parse_basis(source, media)),
        "flex" => assign_flex(&mut style.flex, parse_flex(source, media)),
        "order" => assign(&mut style.order, parse_order(source)),
        _ => return false,
    }
    true
}

fn assign<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn assign_flex(style: &mut FlexStyle, value: Option<FlexStyle>) {
    if let Some(value) = value {
        style.grow = value.grow;
        style.shrink = value.shrink;
        style.basis = value.basis;
    }
}

fn parse_direction(source: &str) -> Option<FlexDirection> {
    match parse_ident(source)?.as_str() {
        "row" => Some(FlexDirection::Row),
        "row-reverse" => Some(FlexDirection::RowReverse),
        "column" => Some(FlexDirection::Column),
        "column-reverse" => Some(FlexDirection::ColumnReverse),
        _ => None,
    }
}

fn parse_wrap(source: &str) -> Option<FlexWrap> {
    match parse_ident(source)?.as_str() {
        "nowrap" => Some(FlexWrap::NoWrap),
        "wrap" => Some(FlexWrap::Wrap),
        "wrap-reverse" => Some(FlexWrap::WrapReverse),
        _ => None,
    }
}

fn assign_flow(style: &mut FlexStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut direction = None;
    let mut wrap = None;
    while !parser.is_exhausted() {
        let Ok(word) = parser.expect_ident_cloned() else {
            return;
        };
        let word = word.to_ascii_lowercase();
        if direction.is_none() {
            direction = match word.as_str() {
                "row" => Some(FlexDirection::Row),
                "row-reverse" => Some(FlexDirection::RowReverse),
                "column" => Some(FlexDirection::Column),
                "column-reverse" => Some(FlexDirection::ColumnReverse),
                _ => None,
            };
            if direction.is_some() {
                continue;
            }
        }
        if wrap.is_none() {
            wrap = match word.as_str() {
                "nowrap" => Some(FlexWrap::NoWrap),
                "wrap" => Some(FlexWrap::Wrap),
                "wrap-reverse" => Some(FlexWrap::WrapReverse),
                _ => None,
            };
            if wrap.is_some() {
                continue;
            }
        }
        return;
    }
    if direction.is_none() && wrap.is_none() {
        return;
    }
    style.direction = direction.unwrap_or_default();
    style.wrap = wrap.unwrap_or_default();
}

fn parse_number(source: &str) -> Option<CssNumber> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_number().ok()?;
    parser.expect_exhausted().ok()?;
    CssNumber::new(value)
}

fn parse_order(source: &str) -> Option<i32> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_integer().ok()?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_basis(source: &str, media: MediaContext) -> Option<FlexBasis> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_basis_parser(&mut parser, media)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_basis_parser(parser: &mut Parser<'_, '_>, media: MediaContext) -> Option<FlexBasis> {
    if let Ok(value) = parser.try_parse(|input| input.expect_ident_cloned()) {
        return match value.to_ascii_lowercase().as_str() {
            "auto" => Some(FlexBasis::Auto),
            "content" => Some(FlexBasis::Content),
            _ => None,
        };
    }
    let state = parser.state();
    if let Ok(Token::Percentage { unit_value, .. }) = parser.next().cloned() {
        return percentage_value(unit_value).map(FlexBasis::Percent);
    }
    parser.reset(&state);
    let length = parse_length_token(parser)?;
    Some(FlexBasis::Cells(AxisCellLength {
        horizontal: media.resolve_cells(length, LengthAxis::Horizontal),
        vertical: media.resolve_cells(length, LengthAxis::Vertical),
    }))
}

fn parse_flex(source: &str, media: MediaContext) -> Option<FlexStyle> {
    match parse_ident(source).as_deref() {
        Some("none") => return Some(FlexStyle::none()),
        Some("auto") => {
            return Some(FlexStyle {
                grow: CssNumber::ONE,
                shrink: CssNumber::ONE,
                basis: FlexBasis::Auto,
                ..Default::default()
            });
        }
        _ => {}
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let first_number = parser.try_parse(|input| input.expect_number()).ok();
    let (grow, shrink, basis) = if let Some(grow) = first_number {
        let grow = CssNumber::new(grow)?;
        let shrink = if let Ok(value) = parser.try_parse(|input| input.expect_number()) {
            CssNumber::new(value)?
        } else {
            CssNumber::ONE
        };
        let basis = if parser.is_exhausted() {
            FlexBasis::zero()
        } else {
            parse_basis_parser(&mut parser, media)?
        };
        (grow, shrink, basis)
    } else {
        (
            CssNumber::ONE,
            CssNumber::ONE,
            parse_basis_parser(&mut parser, media)?,
        )
    };
    parser.expect_exhausted().ok()?;
    Some(FlexStyle {
        grow,
        shrink,
        basis,
        ..Default::default()
    })
}
