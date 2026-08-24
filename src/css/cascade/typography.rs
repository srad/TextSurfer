use cssparser::{Parser, ParserInput, Token};

use crate::core::style::{ComputedStyle, CssLength, CssLengthUnit, FontSize};
use crate::css::Declaration;

use super::MediaContext;

pub(super) fn apply_font_size(
    style: &mut ComputedStyle,
    parent: Option<ComputedStyle>,
    declaration: &Declaration,
    media: MediaContext,
) {
    if declaration.name != "font-size" {
        return;
    }
    let inherited = parent.map_or(FontSize::INITIAL, |style| style.font_size);
    let mut input = ParserInput::new(&declaration.value);
    let mut parser = Parser::new(&mut input);
    let Ok(token) = parser.next().cloned() else {
        return;
    };
    if !parser.is_exhausted() {
        return;
    }
    let px = match token {
        Token::Ident(value) if value.eq_ignore_ascii_case("inherit") => inherited.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("unset") => inherited.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("initial") => FontSize::INITIAL.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("xx-small") => 9.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("x-small") => 10.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("small") => 13.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("medium") => 16.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("large") => 18.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("x-large") => 24.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("xx-large") => 32.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("xxx-large") => 48.0,
        Token::Ident(value) if value.eq_ignore_ascii_case("larger") => inherited.px() * 1.2,
        Token::Ident(value) if value.eq_ignore_ascii_case("smaller") => inherited.px() / 1.2,
        Token::Percentage { unit_value, .. } if unit_value >= 0.0 => {
            inherited.px() * f64::from(unit_value)
        }
        Token::Dimension {
            value, ref unit, ..
        } if value >= 0.0 => {
            let Some(unit) = length_unit(unit) else {
                return;
            };
            let Some(length) = CssLength::new(value, unit) else {
                return;
            };
            media.css_pixels_for_font_size(length, inherited)
        }
        Token::Number { value: 0.0, .. } => 0.0,
        _ => return,
    };
    if let Some(size) = FontSize::from_px(px) {
        style.font_size = size;
    }
}

fn length_unit(unit: &str) -> Option<CssLengthUnit> {
    match unit.to_ascii_lowercase().as_str() {
        "px" => Some(CssLengthUnit::Px),
        "in" => Some(CssLengthUnit::In),
        "cm" => Some(CssLengthUnit::Cm),
        "mm" => Some(CssLengthUnit::Mm),
        "q" => Some(CssLengthUnit::Q),
        "pt" => Some(CssLengthUnit::Pt),
        "pc" => Some(CssLengthUnit::Pc),
        "em" => Some(CssLengthUnit::Em),
        "rem" => Some(CssLengthUnit::Rem),
        "ex" => Some(CssLengthUnit::Ex),
        "ch" => Some(CssLengthUnit::Ch),
        "vw" => Some(CssLengthUnit::Vw),
        "vh" => Some(CssLengthUnit::Vh),
        "vmin" => Some(CssLengthUnit::Vmin),
        "vmax" => Some(CssLengthUnit::Vmax),
        _ => None,
    }
}
