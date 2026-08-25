use cssparser::{ParseError, Parser, ParserInput, Token};

use crate::core::style::{CssCalc, CssLength, CssLengthUnit, LengthAxis};

use super::cascade::MediaContext;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 256;

#[derive(Clone, Copy)]
enum Value {
    Number(f32),
    Length(CssCalc),
}

struct Limits {
    depth: usize,
    nodes: usize,
}

pub(super) fn parse_length_percentage(
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
) -> Option<CssCalc> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut limits = Limits { depth: 0, nodes: 0 };
    let Value::Length(value) = parse_sum(&mut parser, media, axis, &mut limits).ok()? else {
        return None;
    };
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_sum<'i, 't>(
    parser: &mut Parser<'i, 't>,
    media: MediaContext,
    axis: LengthAxis,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    let mut value = parse_product(parser, media, axis, limits)?;
    loop {
        let sign = if parser.try_parse(|input| input.expect_delim('+')).is_ok() {
            1.0
        } else if parser.try_parse(|input| input.expect_delim('-')).is_ok() {
            -1.0
        } else {
            return Ok(value);
        };
        value = add(value, parse_product(parser, media, axis, limits)?, sign)
            .ok_or_else(|| parser.new_custom_error(()))?;
    }
}

fn parse_product<'i, 't>(
    parser: &mut Parser<'i, 't>,
    media: MediaContext,
    axis: LengthAxis,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    let mut value = parse_primary(parser, media, axis, limits)?;
    loop {
        if parser.try_parse(|input| input.expect_delim('*')).is_ok() {
            value = multiply(value, parse_primary(parser, media, axis, limits)?)
                .ok_or_else(|| parser.new_custom_error(()))?;
        } else if parser.try_parse(|input| input.expect_delim('/')).is_ok() {
            value = divide(value, parse_primary(parser, media, axis, limits)?)
                .ok_or_else(|| parser.new_custom_error(()))?;
        } else {
            return Ok(value);
        }
    }
}

fn parse_primary<'i, 't>(
    parser: &mut Parser<'i, 't>,
    media: MediaContext,
    axis: LengthAxis,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    limits.nodes += 1;
    if limits.nodes > MAX_NODES {
        return Err(parser.new_custom_error(()));
    }
    match parser.next()?.clone() {
        Token::Number { value, .. } if value.is_finite() => Ok(Value::Number(value)),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => Ok(Value::Length(
            CssCalc::new(0.0, unit_value).ok_or_else(|| parser.new_custom_error(()))?,
        )),
        Token::Dimension { value, unit, .. } => {
            let length = CssLength::new(
                value,
                length_unit(&unit).ok_or_else(|| parser.new_custom_error(()))?,
            )
            .ok_or_else(|| parser.new_custom_error(()))?;
            Ok(Value::Length(
                CssCalc::new(media.resolve_fractional_cells(length, axis), 0.0)
                    .ok_or_else(|| parser.new_custom_error(()))?,
            ))
        }
        Token::Function(name) => {
            limits.depth += 1;
            if limits.depth > MAX_DEPTH {
                return Err(parser.new_custom_error(()));
            }
            let result = parser.parse_nested_block(|input| {
                if name.eq_ignore_ascii_case("calc") {
                    let value = parse_sum(input, media, axis, limits)?;
                    input.expect_exhausted()?;
                    Ok(value)
                } else if name.eq_ignore_ascii_case("min") || name.eq_ignore_ascii_case("max") {
                    let values =
                        input.parse_comma_separated(|item| parse_sum(item, media, axis, limits))?;
                    range(values, name.eq_ignore_ascii_case("min"))
                        .ok_or_else(|| input.new_custom_error(()))
                } else if name.eq_ignore_ascii_case("clamp") {
                    let values =
                        input.parse_comma_separated(|item| parse_sum(item, media, axis, limits))?;
                    clamp(values).ok_or_else(|| input.new_custom_error(()))
                } else {
                    Err(input.new_custom_error(()))
                }
            });
            limits.depth -= 1;
            result
        }
        Token::ParenthesisBlock => parser.parse_nested_block(|input| {
            let value = parse_sum(input, media, axis, limits)?;
            input.expect_exhausted()?;
            Ok(value)
        }),
        Token::Ident(name) if name.eq_ignore_ascii_case("e") => {
            Ok(Value::Number(std::f32::consts::E))
        }
        Token::Ident(name) if name.eq_ignore_ascii_case("pi") => {
            Ok(Value::Number(std::f32::consts::PI))
        }
        _ => Err(parser.new_custom_error(())),
    }
}

fn add(left: Value, right: Value, sign: f32) -> Option<Value> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => finite(left + sign * right),
        (Value::Length(left), Value::Length(right)) => CssCalc::new(
            left.length() + sign * right.length(),
            left.percent() + sign * right.percent(),
        )
        .map(Value::Length),
        _ => None,
    }
}

fn multiply(left: Value, right: Value) -> Option<Value> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => finite(left * right),
        (Value::Length(value), Value::Number(number))
        | (Value::Number(number), Value::Length(value)) => {
            CssCalc::new(value.length() * number, value.percent() * number).map(Value::Length)
        }
        _ => None,
    }
}

fn divide(left: Value, right: Value) -> Option<Value> {
    let Value::Number(divisor) = right else {
        return None;
    };
    if divisor == 0.0 {
        return None;
    }
    multiply(left, Value::Number(divisor.recip()))
}

fn range(values: Vec<Value>, minimum: bool) -> Option<Value> {
    let mut values = values.into_iter();
    let first = values.next()?;
    values.try_fold(first, |left, right| select(left, right, minimum))
}

fn clamp(values: Vec<Value>) -> Option<Value> {
    let [minimum, value, maximum] = values.as_slice() else {
        return None;
    };
    select(select(*minimum, *value, false)?, *maximum, true)
}

fn select(left: Value, right: Value, minimum: bool) -> Option<Value> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => finite(if minimum {
            left.min(right)
        } else {
            left.max(right)
        }),
        (Value::Length(left), Value::Length(right))
            if left.percent() == right.percent() || left.length() == right.length() =>
        {
            let basis = if left.percent() == right.percent() {
                0.0
            } else {
                1.0
            };
            Some(Value::Length(
                if minimum == (left.resolve(basis) <= right.resolve(basis)) {
                    left
                } else {
                    right
                },
            ))
        }
        _ => None,
    }
}

fn finite(value: f32) -> Option<Value> {
    value.is_finite().then_some(Value::Number(value))
}

fn length_unit(unit: &str) -> Option<CssLengthUnit> {
    use CssLengthUnit::*;
    Some(match unit.to_ascii_lowercase().as_str() {
        "px" => Px,
        "in" => In,
        "cm" => Cm,
        "mm" => Mm,
        "q" => Q,
        "pt" => Pt,
        "pc" => Pc,
        "em" => Em,
        "rem" => Rem,
        "ex" => Ex,
        "ch" => Ch,
        "vw" => Vw,
        "vh" => Vh,
        "vmin" => Vmin,
        "vmax" => Vmax,
        _ => return None,
    })
}
