use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use crate::core::style::{
    Alignment, AlignmentSafety, AxisCellLength, ComputedStyle, ContentAlignment, CssGap, CssNumber,
    CssPercentage, FlexBasis, FlexDirection, FlexStyle, FlexWrap, ItemAlignment, LengthAxis,
};
use crate::css::values::parse_length_token;

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
        "order" => assign(&mut style.flex.order, parse_order(source)),
        "justify-content" => assign(
            &mut style.flex.justify_content,
            parse_content_alignment(source, true),
        ),
        "align-items" => assign(
            &mut style.flex.align_items,
            parse_item_alignment(source, false),
        ),
        "align-self" => assign(&mut style.flex.align_self, parse_align_self(source)),
        "align-content" => assign(
            &mut style.flex.align_content,
            parse_content_alignment(source, false),
        ),
        "row-gap" => assign(
            &mut style.flex.row_gap,
            parse_gap(source, media, LengthAxis::Vertical),
        ),
        "column-gap" => assign(
            &mut style.flex.column_gap,
            parse_gap(source, media, LengthAxis::Horizontal),
        ),
        "gap" => assign_gap(&mut style.flex, source, media),
        "place-content" => assign_place_content(&mut style.flex, source),
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
    match ident(source)?.as_str() {
        "row" => Some(FlexDirection::Row),
        "row-reverse" => Some(FlexDirection::RowReverse),
        "column" => Some(FlexDirection::Column),
        "column-reverse" => Some(FlexDirection::ColumnReverse),
        _ => None,
    }
}

fn parse_wrap(source: &str) -> Option<FlexWrap> {
    match ident(source)?.as_str() {
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
        return percentage(unit_value).map(FlexBasis::Percent);
    }
    parser.reset(&state);
    let length = parse_length_token(parser)?;
    Some(FlexBasis::Cells(AxisCellLength {
        horizontal: media.resolve_cells(length, LengthAxis::Horizontal),
        vertical: media.resolve_cells(length, LengthAxis::Vertical),
    }))
}

fn parse_flex(source: &str, media: MediaContext) -> Option<FlexStyle> {
    match ident(source).as_deref() {
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

fn parse_gap(source: &str, media: MediaContext, axis: LengthAxis) -> Option<CssGap> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_gap_parser(&mut parser, media, axis)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_gap_parser(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
) -> Option<CssGap> {
    if parser
        .try_parse(|input| input.expect_ident_matching("normal"))
        .is_ok()
    {
        return Some(CssGap::Normal);
    }
    let state = parser.state();
    if let Ok(Token::Percentage { unit_value, .. }) = parser.next().cloned() {
        return percentage(unit_value).map(CssGap::Percent);
    }
    parser.reset(&state);
    parse_length_token(parser).map(|length| CssGap::Cells(media.resolve_cells(length, axis)))
}

fn assign_gap(style: &mut FlexStyle, source: &str, media: MediaContext) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let Some(row) = parse_gap_parser(&mut parser, media, LengthAxis::Vertical) else {
        return;
    };
    let column = if parser.is_exhausted() {
        match row {
            CssGap::Cells(_) => {
                let mut input = ParserInput::new(source);
                let mut parser = Parser::new(&mut input);
                let Some(value) = parse_gap_parser(&mut parser, media, LengthAxis::Horizontal)
                else {
                    return;
                };
                value
            }
            value => value,
        }
    } else {
        let Some(value) = parse_gap_parser(&mut parser, media, LengthAxis::Horizontal) else {
            return;
        };
        value
    };
    if parser.expect_exhausted().is_err() {
        return;
    }
    style.row_gap = row;
    style.column_gap = column;
}

fn parse_item_alignment(source: &str, allow_auto: bool) -> Option<Alignment<ItemAlignment>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_item_alignment_parser(&mut parser, allow_auto)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_item_alignment_parser(
    parser: &mut Parser<'_, '_>,
    allow_auto: bool,
) -> Option<Alignment<ItemAlignment>> {
    let first = parser.expect_ident_cloned().ok()?.to_ascii_lowercase();
    let (safety, word) = match first.as_str() {
        "safe" => (
            AlignmentSafety::Safe,
            parser.expect_ident_cloned().ok()?.to_ascii_lowercase(),
        ),
        "unsafe" => (
            AlignmentSafety::Unsafe,
            parser.expect_ident_cloned().ok()?.to_ascii_lowercase(),
        ),
        _ => (AlignmentSafety::Unsafe, first),
    };
    let keyword = match word.as_str() {
        "auto" if allow_auto => return None,
        "normal" => ItemAlignment::Normal,
        "stretch" if safety == AlignmentSafety::Unsafe => ItemAlignment::Stretch,
        "start" => ItemAlignment::Start,
        "end" => ItemAlignment::End,
        "flex-start" => ItemAlignment::FlexStart,
        "flex-end" => ItemAlignment::FlexEnd,
        "self-start" => ItemAlignment::SelfStart,
        "self-end" => ItemAlignment::SelfEnd,
        "center" => ItemAlignment::Center,
        "baseline" if safety == AlignmentSafety::Unsafe => ItemAlignment::Baseline,
        _ => return None,
    };
    Some(Alignment { keyword, safety })
}

fn parse_align_self(source: &str) -> Option<Option<Alignment<ItemAlignment>>> {
    if ident(source).as_deref() == Some("auto") {
        Some(None)
    } else {
        parse_item_alignment(source, false).map(Some)
    }
}

fn parse_content_alignment(source: &str, physical: bool) -> Option<Alignment<ContentAlignment>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_content_alignment_parser(&mut parser, physical)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_content_alignment_parser(
    parser: &mut Parser<'_, '_>,
    physical: bool,
) -> Option<Alignment<ContentAlignment>> {
    let first = parser.expect_ident_cloned().ok()?.to_ascii_lowercase();
    let (safety, word) = match first.as_str() {
        "safe" => (
            AlignmentSafety::Safe,
            parser.expect_ident_cloned().ok()?.to_ascii_lowercase(),
        ),
        "unsafe" => (
            AlignmentSafety::Unsafe,
            parser.expect_ident_cloned().ok()?.to_ascii_lowercase(),
        ),
        _ => (AlignmentSafety::Unsafe, first),
    };
    let keyword = match word.as_str() {
        "normal" if safety == AlignmentSafety::Unsafe => ContentAlignment::Normal,
        "stretch" if safety == AlignmentSafety::Unsafe => ContentAlignment::Stretch,
        "start" => ContentAlignment::Start,
        "end" => ContentAlignment::End,
        "flex-start" => ContentAlignment::FlexStart,
        "flex-end" => ContentAlignment::FlexEnd,
        "center" => ContentAlignment::Center,
        "space-between" if safety == AlignmentSafety::Unsafe => ContentAlignment::SpaceBetween,
        "space-around" if safety == AlignmentSafety::Unsafe => ContentAlignment::SpaceAround,
        "space-evenly" if safety == AlignmentSafety::Unsafe => ContentAlignment::SpaceEvenly,
        "left" if physical => ContentAlignment::Left,
        "right" if physical => ContentAlignment::Right,
        _ => return None,
    };
    Some(Alignment { keyword, safety })
}

fn assign_place_content(style: &mut FlexStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let Some(align) = parse_content_alignment_parser(&mut parser, false) else {
        return;
    };
    let justify = if parser.is_exhausted() {
        align
    } else {
        let Some(value) = parse_content_alignment_parser(&mut parser, true) else {
            return;
        };
        value
    };
    if parser.expect_exhausted().is_err() {
        return;
    }
    style.align_content = align;
    style.justify_content = justify;
}

fn percentage(value: f32) -> Option<CssPercentage> {
    (value.is_finite() && value >= 0.0)
        .then(|| CssPercentage::new((value * 10_000.0).round().min(u32::MAX as f32) as u32))
}

fn ident(source: &str) -> Option<String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_ident_cloned().ok()?.to_ascii_lowercase();
    parser.expect_exhausted().ok()?;
    Some(value)
}
