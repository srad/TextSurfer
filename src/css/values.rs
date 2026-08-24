//! CSS value grammar: the token-level readers that map an already-tokenized declaration value onto
//! a `ComputedStyle` field. A peer of `css::parser`, not part of applying the cascade.

use cssparser::color::clamp_unit_f32;
use cssparser::{Parser, ParserInput, Token};
use cssparser_color::{Color as CssColor, hsl_to_rgb, hwb_to_rgb};

use crate::core::geom::Size;
use crate::core::style::{
    BorderColor, BorderEdges, BorderLineStyle, BorderSide, CellMetric, CssLength, CssLengthUnit,
    CssPercentage, CssWidth, Display, DisplayBox, DisplayInternal, DisplayOutside, EdgeSizes,
    LengthAxis, ListStylePosition, ListStyleType, Rgb, Rgba,
};

pub(super) fn parse_display(source: &str) -> Option<Display> {
    enum ParsedInside {
        Flow(bool),
        Table,
        Flex,
        Grid,
    }

    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut words = Vec::new();
    while !parser.is_exhausted() {
        words.push(parser.expect_ident_cloned().ok()?.to_ascii_lowercase());
    }
    if words.len() == 1 {
        return match words[0].as_str() {
            "none" => Some(Display::Box(DisplayBox::None)),
            "contents" => Some(Display::Box(DisplayBox::Contents)),
            "block" | "flow" => Some(Display::BLOCK),
            "inline" => Some(Display::INLINE),
            "flow-root" => Some(Display::FLOW_ROOT),
            "list-item" => Some(Display::LIST_ITEM),
            "table" => Some(Display::TABLE),
            "inline-block" => Some(Display::INLINE_BLOCK),
            "inline-table" => Some(Display::INLINE_TABLE),
            "flex" => Some(Display::flex(DisplayOutside::Block)),
            "inline-flex" => Some(Display::flex(DisplayOutside::Inline)),
            "grid" => Some(Display::grid(DisplayOutside::Block)),
            "inline-grid" => Some(Display::grid(DisplayOutside::Inline)),
            "table-header-group" => Some(Display::Internal(DisplayInternal::TableHeaderGroup)),
            "table-row-group" => Some(Display::Internal(DisplayInternal::TableRowGroup)),
            "table-footer-group" => Some(Display::Internal(DisplayInternal::TableFooterGroup)),
            "table-row" => Some(Display::Internal(DisplayInternal::TableRow)),
            "table-cell" => Some(Display::Internal(DisplayInternal::TableCell)),
            "table-column" => Some(Display::Internal(DisplayInternal::TableColumn)),
            "table-column-group" => Some(Display::Internal(DisplayInternal::TableColumnGroup)),
            "table-caption" => Some(Display::Internal(DisplayInternal::TableCaption)),
            _ => None,
        };
    }
    let mut outside = None;
    let mut inside: Option<ParsedInside> = None;
    let mut list_item = false;
    for word in words {
        match word.as_str() {
            "block" if outside.is_none() => outside = Some(DisplayOutside::Block),
            "inline" if outside.is_none() => outside = Some(DisplayOutside::Inline),
            "flow" if inside.is_none() => inside = Some(ParsedInside::Flow(false)),
            "flow-root" if inside.is_none() => inside = Some(ParsedInside::Flow(true)),
            "list-item" if !list_item => list_item = true,
            "table" if inside.is_none() => inside = Some(ParsedInside::Table),
            "flex" if inside.is_none() => inside = Some(ParsedInside::Flex),
            "grid" if inside.is_none() => inside = Some(ParsedInside::Grid),
            _ => return None,
        }
    }
    let outside = outside.unwrap_or(DisplayOutside::Block);
    match inside.unwrap_or(ParsedInside::Flow(false)) {
        ParsedInside::Flow(flow_root) => Some(Display::flow(outside, flow_root, list_item)),
        ParsedInside::Table if !list_item => Some(Display::table(outside)),
        ParsedInside::Flex if !list_item => Some(Display::flex(outside)),
        ParsedInside::Grid if !list_item => Some(Display::grid(outside)),
        ParsedInside::Table | ParsedInside::Flex | ParsedInside::Grid => None,
    }
}

pub(super) fn parse_color(source: &str) -> Option<Rgba> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let color = CssColor::parse(&mut parser).ok()?;
    parser.expect_exhausted().ok()?;
    css_color_to_rgba(color)
}

pub(super) fn parse_background_color(source: &str) -> Option<Option<Rgb>> {
    let color = parse_color(source)?;
    Some((color.alpha > 0).then_some(color.rgb))
}

fn css_color_to_rgba(color: CssColor) -> Option<Rgba> {
    let ((r, g, b), alpha) = match color {
        CssColor::Rgba(rgba) => ((rgba.red, rgba.green, rgba.blue), rgba.alpha),
        CssColor::Hsl(hsl) => (
            float_rgb(hsl_to_rgb(
                hsl.hue.unwrap_or_default() / 360.0,
                hsl.saturation.unwrap_or_default(),
                hsl.lightness.unwrap_or_default(),
            )),
            hsl.alpha.unwrap_or(1.0),
        ),
        CssColor::Hwb(hwb) => (
            float_rgb(hwb_to_rgb(
                hwb.hue.unwrap_or_default() / 360.0,
                hwb.whiteness.unwrap_or_default(),
                hwb.blackness.unwrap_or_default(),
            )),
            hwb.alpha.unwrap_or(1.0),
        ),
        _ => return None,
    };
    Some(Rgba::new(r, g, b, clamp_unit_f32(alpha)))
}

fn css_color_to_rgb(color: CssColor) -> Option<Rgb> {
    css_color_to_rgba(color).map(|color| color.rgb)
}

fn float_rgb(components: (f32, f32, f32)) -> (u8, u8, u8) {
    let (red, green, blue) = components;
    (
        clamp_unit_f32(red),
        clamp_unit_f32(green),
        clamp_unit_f32(blue),
    )
}

pub(super) fn parse_font_weight(source: &str) -> Option<bool> {
    if let Some(keyword) = parse_ident(source) {
        return match keyword.as_str() {
            "bold" | "bolder" => Some(true),
            "normal" | "lighter" => Some(false),
            _ => None,
        };
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let weight = parser.expect_number().ok()?;
    parser.expect_exhausted().ok()?;
    Some(weight > 500.0)
}

pub(super) fn parse_text_decoration(source: &str) -> Option<(bool, bool)> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut underline = false;
    let mut strike = false;
    let mut seen = false;
    while let Ok(token) = parser.next() {
        let Token::Ident(value) = token else {
            continue;
        };
        seen = true;
        match value.to_ascii_lowercase().as_str() {
            "none" | "initial" => return Some((false, false)),
            "underline" | "overline" => underline = true,
            "line-through" => strike = true,
            _ => {}
        }
    }
    seen.then_some((underline, strike))
}

pub(super) fn parse_ident(source: &str) -> Option<String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_ident_cloned().ok()?;
    parser.expect_exhausted().ok()?;
    Some(value.to_ascii_lowercase())
}

pub(super) fn is_css_wide_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "initial" | "inherit" | "unset" | "revert" | "none"
    )
}

pub(super) fn parse_list_style_position(value: &str) -> Option<ListStylePosition> {
    match value.to_ascii_lowercase().as_str() {
        "outside" => Some(ListStylePosition::Outside),
        "inside" => Some(ListStylePosition::Inside),
        _ => None,
    }
}

pub(super) fn parse_list_style_type(value: &str) -> Option<ListStyleType> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(ListStyleType::None),
        "disc" => Some(ListStyleType::Disc),
        "circle" => Some(ListStyleType::Circle),
        "square" => Some(ListStyleType::Square),
        "decimal" => Some(ListStyleType::Decimal),
        "decimal-leading-zero" => Some(ListStyleType::DecimalLeadingZero),
        "lower-alpha" | "lower-latin" => Some(ListStyleType::LowerAlpha),
        "upper-alpha" | "upper-latin" => Some(ListStyleType::UpperAlpha),
        "lower-roman" => Some(ListStyleType::LowerRoman),
        "upper-roman" => Some(ListStyleType::UpperRoman),
        _ => None,
    }
}

pub(super) fn consume_block<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    while input.next().is_ok() {}
    Ok(())
}

pub(super) fn parse_border(source: &str) -> Option<BorderEdges> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut side = BorderSide::default();
    let mut saw_width = false;
    let mut saw_style = false;
    let mut saw_color = false;
    while !parser.is_exhausted() {
        if !saw_color && let Ok(color) = parser.try_parse(CssColor::parse) {
            side.color = border_color(color)?;
            saw_color = true;
            continue;
        }
        let token = parser.next().ok()?;
        match token {
            Token::Ident(value) => {
                if !saw_style && let Some(border_style) = parse_border_style_ident(value) {
                    side.style = border_style;
                    saw_style = true;
                } else if !saw_width && let Some(width) = parse_border_width_ident(value) {
                    side.width = width;
                    saw_width = true;
                } else {
                    return None;
                }
            }
            token if !saw_width => {
                let width = length_from_token(token)?;
                if width.value() < 0.0 {
                    return None;
                }
                side.width = usize::from(width.value() > 0.0);
                saw_width = true;
            }
            _ => return None,
        }
    }
    (saw_width || saw_style || saw_color).then_some(BorderEdges::uniform(side))
}

fn border_color(color: CssColor) -> Option<BorderColor> {
    match color {
        CssColor::CurrentColor => Some(BorderColor::CurrentColor),
        CssColor::Rgba(rgba) if rgba.alpha <= 0.0 => Some(BorderColor::Transparent),
        color => css_color_to_rgb(color).map(BorderColor::Rgb),
    }
}

fn parse_border_style_ident(value: &str) -> Option<BorderLineStyle> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(BorderLineStyle::None),
        "hidden" => Some(BorderLineStyle::Hidden),
        "inset" => Some(BorderLineStyle::Inset),
        "groove" => Some(BorderLineStyle::Groove),
        "outset" => Some(BorderLineStyle::Outset),
        "ridge" => Some(BorderLineStyle::Ridge),
        "dotted" => Some(BorderLineStyle::Dotted),
        "dashed" => Some(BorderLineStyle::Dashed),
        "solid" => Some(BorderLineStyle::Solid),
        "double" => Some(BorderLineStyle::Double),
        _ => None,
    }
}

fn parse_border_width_ident(value: &str) -> Option<usize> {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "thin" | "medium" | "thick"
    )
    .then_some(1)
}

fn parse_border_styles(source: &str) -> Option<Vec<BorderLineStyle>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(parse_border_style_ident(parser.expect_ident().ok()?)?);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn parse_border_widths(source: &str) -> Option<Vec<usize>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        let value = match parser.next().ok()? {
            Token::Ident(value) => parse_border_width_ident(value)?,
            token => {
                let width = length_from_token(token)?;
                if width.value() < 0.0 {
                    return None;
                }
                usize::from(width.value() > 0.0)
            }
        };
        values.push(value);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn parse_border_colors(source: &str) -> Option<Vec<BorderColor>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(border_color(CssColor::parse(&mut parser).ok()?)?);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn expanded_edges<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    match values {
        [all] => Some([*all; 4]),
        [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

pub(super) fn assign_border_side(target: &mut BorderSide, value: &str) {
    if let Some(border) = parse_border(value) {
        *target = border.top;
    }
}

pub(super) fn assign_border_styles(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_styles(value).and_then(|values| expanded_edges(&values)) {
        target.top.style = values[0];
        target.right.style = values[1];
        target.bottom.style = values[2];
        target.left.style = values[3];
    }
}

pub(super) fn assign_border_style(target: &mut BorderSide, value: &str) {
    if let Some(value) = parse_ident(value).and_then(|value| parse_border_style_ident(&value)) {
        target.style = value;
    }
}

pub(super) fn assign_border_widths(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_widths(value).and_then(|values| expanded_edges(&values)) {
        target.top.width = values[0];
        target.right.width = values[1];
        target.bottom.width = values[2];
        target.left.width = values[3];
    }
}

pub(super) fn assign_border_width(target: &mut BorderSide, value: &str) {
    if let Some(values) = parse_border_widths(value)
        && let [width] = values.as_slice()
    {
        target.width = *width;
    }
}

pub(super) fn assign_border_colors(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_colors(value).and_then(|values| expanded_edges(&values)) {
        target.top.color = values[0];
        target.right.color = values[1];
        target.bottom.color = values[2];
        target.left.color = values[3];
    }
}

pub(super) fn assign_border_color(target: &mut BorderSide, value: &str) {
    if let Some(values) = parse_border_colors(value)
        && let [color] = values.as_slice()
    {
        target.color = *color;
    }
}

pub(super) fn assign_one(
    target: &mut usize,
    value: &str,
    axis: LengthAxis,
    metric: CellMetric,
    viewport: Size,
) {
    if let Some(value) = parse_length(value) {
        *target = metric.resolve_cells(value, axis, viewport);
    }
}

pub(super) fn assign_edges(
    edges: &mut EdgeSizes,
    values: &[CssLength],
    metric: CellMetric,
    viewport: Size,
) {
    let values = expanded_edges(values).map(|values| {
        [
            metric.resolve_cells(values[0], LengthAxis::Vertical, viewport),
            metric.resolve_cells(values[1], LengthAxis::Horizontal, viewport),
            metric.resolve_cells(values[2], LengthAxis::Vertical, viewport),
            metric.resolve_cells(values[3], LengthAxis::Horizontal, viewport),
        ]
    });
    if let Some([top, right, bottom, left]) = values.as_ref() {
        *edges = EdgeSizes {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        };
    }
}

pub(super) fn parse_width(source: &str, metric: CellMetric, viewport: Size) -> Option<CssWidth> {
    if parse_ident(source).as_deref() == Some("auto") {
        return Some(CssWidth::Auto);
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let percentage = match parser.next() {
        Ok(Token::Percentage { unit_value, .. }) => Some(*unit_value),
        _ => None,
    };
    if let Some(unit_value) = percentage
        && unit_value.is_finite()
        && unit_value >= 0.0
        && parser.is_exhausted()
    {
        return Some(CssWidth::Percent(CssPercentage::new(
            (unit_value * 10_000.0).round().min(u32::MAX as f32) as u32,
        )));
    }
    parse_length(source)
        .map(|value| CssWidth::Cells(metric.resolve_cells(value, LengthAxis::Horizontal, viewport)))
}

fn parse_length(source: &str) -> Option<CssLength> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_length_token(&mut parser)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

pub(super) fn parse_lengths(source: &str) -> Option<Vec<CssLength>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() {
        if values.len() == 4 {
            return None;
        }
        values.push(parse_length_token(&mut parser)?);
    }
    (!values.is_empty()).then_some(values)
}

pub(super) fn parse_length_token(parser: &mut Parser<'_, '_>) -> Option<CssLength> {
    let length = length_from_token(parser.next().ok()?)?;
    (length.value() >= 0.0).then_some(length)
}

pub(super) fn parse_signed_length_token(parser: &mut Parser<'_, '_>) -> Option<CssLength> {
    length_from_token(parser.next().ok()?)
}

fn length_from_token(token: &Token<'_>) -> Option<CssLength> {
    match token {
        Token::Number { value, .. } if *value == 0.0 => Some(CssLength::zero()),
        Token::Dimension { value, unit, .. } => CssLength::new(*value, parse_length_unit(unit)?),
        _ => None,
    }
}

fn parse_length_unit(unit: &str) -> Option<CssLengthUnit> {
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
