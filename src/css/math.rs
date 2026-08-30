use cssparser::{ParseError, Parser, ParserInput, Token};

use crate::core::geom::Size;
use crate::core::style::{CellMetric, CssCalcExpr, CssLength, CssLengthUnit, LengthAxis};

#[cfg(test)]
use super::cascade::MediaContext;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 256;

#[derive(Clone)]
enum Value {
    Number(f32),
    Length(ParsedMath),
}

#[derive(Clone)]
pub(super) enum ParsedMath {
    Length(CssLength),
    Percent(f32),
    Sum(Vec<(bool, Self)>),
    Scale {
        value: Box<Self>,
        factor: f32,
    },
    Min(Vec<Self>),
    Max(Vec<Self>),
    Clamp {
        minimum: Option<Box<Self>>,
        value: Box<Self>,
        maximum: Option<Box<Self>>,
    },
}

impl ParsedMath {
    #[cfg(test)]
    pub(super) fn lower_cells(
        &self,
        media: MediaContext,
        output_axis: LengthAxis,
        basis_axis: LengthAxis,
    ) -> Option<CssCalcExpr> {
        self.lower(
            &|length| media.resolve_fractional_cells(length, output_axis),
            percent_scale(media.cell_metric, output_axis, basis_axis),
        )
    }

    /// [`Self::lower_cells`] for a caller that has no [`MediaContext`].
    ///
    /// The Stylo mapper (`css::stylo::map`) reaches this with expressions Stylo has already
    /// computed, so the font-relative units `MediaContext` exists to resolve cannot appear. The
    /// cell metric and viewport are the whole of what is left. Both entry points share the
    /// percentage scale, which is the part that is easy to get wrong.
    pub(in crate::css) fn lower_cells_with_metric(
        &self,
        cell: CellMetric,
        viewport: Size,
        output_axis: LengthAxis,
        basis_axis: LengthAxis,
    ) -> Option<CssCalcExpr> {
        let cell_px = f64::from(match output_axis {
            LengthAxis::Horizontal => cell.column_px(),
            LengthAxis::Vertical => cell.row_px(),
        });
        self.lower(
            &|length| (cell.css_pixels(length, viewport) / cell_px) as f32,
            percent_scale(cell, output_axis, basis_axis),
        )
    }

    fn lower(
        &self,
        resolve_length: &impl Fn(CssLength) -> f32,
        percent_scale: f32,
    ) -> Option<CssCalcExpr> {
        match self {
            Self::Length(length) => CssCalcExpr::linear(resolve_length(*length), 0.0),
            Self::Percent(value) => {
                CssCalcExpr::linear_with_basis(0.0, value * percent_scale, true)
            }
            Self::Sum(values) => {
                let mut values = values.iter();
                let (positive, first) = values.next()?;
                let mut result = if *positive {
                    first.lower(resolve_length, percent_scale)?
                } else {
                    first.lower(resolve_length, percent_scale)?.scale(-1.0)?
                };
                for (positive, value) in values {
                    result = result.add(value.lower(resolve_length, percent_scale)?, *positive)?;
                }
                Some(result)
            }
            Self::Scale { value, factor } => {
                value.lower(resolve_length, percent_scale)?.scale(*factor)
            }
            Self::Min(values) => CssCalcExpr::minimum(
                values
                    .iter()
                    .map(|value| value.lower(resolve_length, percent_scale))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Max(values) => CssCalcExpr::maximum(
                values
                    .iter()
                    .map(|value| value.lower(resolve_length, percent_scale))
                    .collect::<Option<Vec<_>>>()?,
            ),
            Self::Clamp {
                minimum,
                value,
                maximum,
            } => {
                let minimum = match minimum.as_deref() {
                    Some(value) => Some(value.lower(resolve_length, percent_scale)?),
                    None => None,
                };
                let maximum = match maximum.as_deref() {
                    Some(value) => Some(value.lower(resolve_length, percent_scale)?),
                    None => None,
                };
                Some(CssCalcExpr::clamp(
                    minimum,
                    value.lower(resolve_length, percent_scale)?,
                    maximum,
                ))
            }
        }
    }
}

struct Limits {
    depth: usize,
    nodes: usize,
}

/// What a percentage is multiplied by once its basis is expressed in output-axis cells.
///
/// A percentage resolves against a basis measured on `basis_axis` but the expression is evaluated
/// in `output_axis` cells, so a vertical margin of `50%` — basis width, output rows — is scaled by
/// `column_px / row_px`. Getting this wrong is invisible to the type system and off by the cell
/// aspect ratio.
fn percent_scale(cell: CellMetric, output_axis: LengthAxis, basis_axis: LengthAxis) -> f32 {
    let axis_px = |axis| match axis {
        LengthAxis::Horizontal => cell.column_px(),
        LengthAxis::Vertical => cell.row_px(),
    };
    f32::from(axis_px(basis_axis)) / f32::from(axis_px(output_axis))
}

#[cfg(test)]
pub(super) fn parse_length_percentage(
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
) -> Option<CssCalcExpr> {
    parse_length_percentage_source(source)?.lower_cells(media, axis, axis)
}

/// Parse a whole `<length-percentage>` — bare length, bare percentage, or math function — without
/// lowering it.
///
/// [`parse_math`] is not a substitute: it requires a function token, so it rejects the bare `10px`
/// a Stylo `calc()` whose node collapsed to a single leaf serialises to.
pub(in crate::css) fn parse_length_percentage_source(source: &str) -> Option<ParsedMath> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut limits = Limits { depth: 0, nodes: 0 };
    let Value::Length(value) = parse_sum(&mut parser, &mut limits).ok()? else {
        return None;
    };
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_sum<'i, 't>(
    parser: &mut Parser<'i, 't>,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    let mut value = parse_product(parser, limits)?;
    loop {
        let sign = if parser.try_parse(|input| input.expect_delim('+')).is_ok() {
            1.0
        } else if parser.try_parse(|input| input.expect_delim('-')).is_ok() {
            -1.0
        } else {
            return Ok(value);
        };
        value = add(value, parse_product(parser, limits)?, sign)
            .ok_or_else(|| parser.new_custom_error(()))?;
    }
}

fn parse_product<'i, 't>(
    parser: &mut Parser<'i, 't>,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    let mut value = parse_primary(parser, limits)?;
    loop {
        if parser.try_parse(|input| input.expect_delim('*')).is_ok() {
            value = multiply(value, parse_primary(parser, limits)?)
                .ok_or_else(|| parser.new_custom_error(()))?;
        } else if parser.try_parse(|input| input.expect_delim('/')).is_ok() {
            value = divide(value, parse_primary(parser, limits)?)
                .ok_or_else(|| parser.new_custom_error(()))?;
        } else {
            return Ok(value);
        }
    }
}

fn parse_primary<'i, 't>(
    parser: &mut Parser<'i, 't>,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    limits.nodes += 1;
    if limits.nodes > MAX_NODES {
        return Err(parser.new_custom_error(()));
    }
    match parser.next()?.clone() {
        Token::Number { value, .. } if value.is_finite() => Ok(Value::Number(value)),
        Token::Percentage { unit_value, .. } if unit_value.is_finite() => {
            Ok(Value::Length(ParsedMath::Percent(unit_value)))
        }
        Token::Dimension { value, unit, .. } => {
            let length = CssLength::new(
                value,
                length_unit(&unit).ok_or_else(|| parser.new_custom_error(()))?,
            )
            .ok_or_else(|| parser.new_custom_error(()))?;
            Ok(Value::Length(ParsedMath::Length(length)))
        }
        Token::Function(name) => {
            limits.depth += 1;
            if limits.depth > MAX_DEPTH {
                return Err(parser.new_custom_error(()));
            }
            let result = parser.parse_nested_block(|input| {
                if name.eq_ignore_ascii_case("calc") {
                    let value = parse_sum(input, limits)?;
                    input.expect_exhausted()?;
                    Ok(value)
                } else if name.eq_ignore_ascii_case("min") || name.eq_ignore_ascii_case("max") {
                    let values = input.parse_comma_separated(|item| parse_sum(item, limits))?;
                    range(values, name.eq_ignore_ascii_case("min"))
                        .ok_or_else(|| input.new_custom_error(()))
                } else if name.eq_ignore_ascii_case("clamp") {
                    parse_clamp(input, limits)
                } else {
                    Err(input.new_custom_error(()))
                }
            });
            limits.depth -= 1;
            result
        }
        Token::ParenthesisBlock => {
            limits.depth += 1;
            if limits.depth > MAX_DEPTH {
                return Err(parser.new_custom_error(()));
            }
            let result = parser.parse_nested_block(|input| {
                let value = parse_sum(input, limits)?;
                input.expect_exhausted()?;
                Ok(value)
            });
            limits.depth -= 1;
            result
        }
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
        (Value::Length(left), Value::Length(right)) => Some(Value::Length(ParsedMath::Sum(vec![
            (true, left),
            (sign > 0.0, right),
        ]))),
        _ => None,
    }
}

fn multiply(left: Value, right: Value) -> Option<Value> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => finite(left * right),
        (Value::Length(value), Value::Number(number))
        | (Value::Number(number), Value::Length(value)) => {
            number
                .is_finite()
                .then_some(Value::Length(ParsedMath::Scale {
                    value: Box::new(value),
                    factor: number,
                }))
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
    let mut numbers = Vec::new();
    let mut lengths = Vec::new();
    for value in values {
        match value {
            Value::Number(value) if lengths.is_empty() => numbers.push(value),
            Value::Length(value) if numbers.is_empty() => lengths.push(value),
            _ => return None,
        }
    }
    if !numbers.is_empty() {
        let initial = if minimum {
            f32::INFINITY
        } else {
            f32::NEG_INFINITY
        };
        return finite(numbers.into_iter().fold(initial, |result, value| {
            if minimum {
                result.min(value)
            } else {
                result.max(value)
            }
        }));
    }
    if minimum {
        Some(Value::Length(ParsedMath::Min(lengths)))
    } else {
        Some(Value::Length(ParsedMath::Max(lengths)))
    }
}

fn parse_clamp<'i, 't>(
    parser: &mut Parser<'i, 't>,
    limits: &mut Limits,
) -> Result<Value, ParseError<'i, ()>> {
    let minimum = parse_optional_bound(parser, limits)?;
    parser.expect_comma()?;
    let value = parse_sum(parser, limits)?;
    parser.expect_comma()?;
    let maximum = parse_optional_bound(parser, limits)?;
    parser.expect_exhausted()?;
    clamp(minimum, value, maximum).ok_or_else(|| parser.new_custom_error(()))
}

fn parse_optional_bound<'i, 't>(
    parser: &mut Parser<'i, 't>,
    limits: &mut Limits,
) -> Result<Option<Value>, ParseError<'i, ()>> {
    if parser
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_ok()
    {
        Ok(None)
    } else {
        parse_sum(parser, limits).map(Some)
    }
}

fn clamp(minimum: Option<Value>, value: Value, maximum: Option<Value>) -> Option<Value> {
    match (minimum, value, maximum) {
        (Some(Value::Number(minimum)), Value::Number(value), Some(Value::Number(maximum))) => {
            finite(minimum.max(value.min(maximum)))
        }
        (Some(Value::Number(minimum)), Value::Number(value), None) => finite(minimum.max(value)),
        (None, Value::Number(value), Some(Value::Number(maximum))) => finite(value.min(maximum)),
        (None, Value::Number(value), None) => finite(value),
        (minimum, Value::Length(value), maximum) => {
            let minimum = match minimum {
                Some(Value::Length(value)) => Some(value),
                None => None,
                _ => return None,
            };
            let maximum = match maximum {
                Some(Value::Length(value)) => Some(value),
                None => None,
                _ => return None,
            };
            Some(Value::Length(ParsedMath::Clamp {
                minimum: minimum.map(Box::new),
                value: Box::new(value),
                maximum: maximum.map(Box::new),
            }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::style::{CalcRange, CssCalcStore};

    fn resolve(source: &str, basis: f32) -> Option<f32> {
        let expression =
            parse_length_percentage(source, MediaContext::screen(), LengthAxis::Horizontal)?;
        let mut calculations = CssCalcStore::default();
        let value = calculations.insert(expression, CalcRange::Unbounded)?;
        calculations.resolve(value, basis)
    }

    #[test]
    fn comparison_functions_keep_type_and_used_value_semantics() {
        assert_eq!(resolve("min(75%, 6ch)", 4.0), Some(3.0));
        assert_eq!(resolve("min(75%, 6ch)", 12.0), Some(6.0));
        assert_eq!(resolve("clamp(8ch, 50%, 4ch)", 10.0), Some(8.0));
        assert_eq!(resolve("clamp(none, 50%, none)", 10.0), Some(5.0));
        assert_eq!(resolve("calc(min(75%, 6ch) + 1ch)", 12.0), Some(7.0));
    }

    #[test]
    fn incompatible_or_exhausted_math_is_invalid_as_one_value() {
        assert_eq!(resolve("min(1, 1ch)", 10.0), None);
        assert_eq!(resolve("calc(1ch * 1ch)", 10.0), None);
        assert_eq!(resolve("calc(1ch / 0)", 10.0), None);
        assert_eq!(resolve("clamp(1ch, none, 2ch)", 10.0), None);
        let nested = format!("calc({}1ch{})", "(".repeat(33), ")".repeat(33));
        assert_eq!(resolve(&nested, 10.0), None);
        let wide = format!("calc({})", vec!["1ch"; 257].join(" + "));
        assert_eq!(resolve(&wide, 10.0), None);
    }

    #[test]
    fn identical_expressions_share_one_style_tree_value() {
        let first = parse_length_percentage(
            "min(75%, 6ch)",
            MediaContext::screen(),
            LengthAxis::Horizontal,
        )
        .unwrap();
        let second = first.clone();
        let mut calculations = CssCalcStore::default();
        assert_eq!(
            calculations.insert(first, CalcRange::Unbounded),
            calculations.insert(second, CalcRange::Unbounded)
        );
    }
}
