use cssparser::{Parser, ParserInput, Token};

use crate::core::style::{ComputedStyle, CssLength, CssLengthUnit, FontSize};
use crate::css::Declaration;

use super::MediaContext;

pub(super) fn apply_font_size(
    style: &mut ComputedStyle,
    parent: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    declaration: &Declaration,
    media: MediaContext,
) {
    if declaration.name != "font-size" {
        return;
    }
    let inherited = parent.map_or(FontSize::INITIAL, |style| style.font_size);
    let Some(px) = parse_font_size(&declaration.value, inherited, ua_style.font_size, media) else {
        return;
    };
    if let Some(size) = FontSize::from_px(px) {
        style.font_size = size;
    }
}

pub(super) fn font_size_value_is_valid(source: &str, media: MediaContext) -> bool {
    parse_font_size(source, FontSize::INITIAL, FontSize::INITIAL, media).is_some()
}

fn parse_font_size(
    source: &str,
    inherited: FontSize,
    ua: FontSize,
    media: MediaContext,
) -> Option<f64> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let token = parser.next().cloned().ok()?;
    if !parser.is_exhausted() {
        return None;
    }
    let px = match token {
        Token::Ident(value) if value.eq_ignore_ascii_case("inherit") => inherited.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("unset") => inherited.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("initial") => FontSize::INITIAL.px(),
        Token::Ident(value) if value.eq_ignore_ascii_case("revert") => ua.px(),
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
            let unit = length_unit(unit)?;
            let length = CssLength::new(value, unit)?;
            media.css_pixels_for_font_size(length, inherited)
        }
        Token::Number { value: 0.0, .. } => 0.0,
        _ => return None,
    };
    FontSize::from_px(px).map(|_| px)
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
