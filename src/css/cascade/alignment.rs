use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use crate::core::style::{
    Alignment, AlignmentSafety, AlignmentStyle, ComputedStyle, ContentAlignment, CssGap,
    ItemAlignment, LengthAxis,
};
use crate::css::values::{parse_ident, parse_length_token, percentage_value};

/// The CSS Box Alignment properties, plus the gap properties they travel with. Both the flex and
/// the grid formatting contexts read these, so they cascade independently of either.
pub(super) fn apply_alignment_declaration(
    style: &mut ComputedStyle,
    property: &str,
    source: &str,
    media: MediaContext,
) -> bool {
    match property {
        "justify-content" => assign(
            &mut style.alignment.justify_content,
            parse_content_alignment(source, true),
        ),
        "align-content" => assign(
            &mut style.alignment.align_content,
            parse_content_alignment(source, false),
        ),
        "justify-items" => assign(
            &mut style.alignment.justify_items,
            parse_item_alignment(source, true),
        ),
        "align-items" => assign(
            &mut style.alignment.align_items,
            parse_item_alignment(source, false),
        ),
        "justify-self" => assign(
            &mut style.alignment.justify_self,
            parse_self_alignment(source, true),
        ),
        "align-self" => assign(
            &mut style.alignment.align_self,
            parse_self_alignment(source, false),
        ),
        "row-gap" | "grid-row-gap" => assign(
            &mut style.alignment.row_gap,
            parse_gap(source, media, LengthAxis::Vertical),
        ),
        "column-gap" | "grid-column-gap" => assign(
            &mut style.alignment.column_gap,
            parse_gap(source, media, LengthAxis::Horizontal),
        ),
        "gap" | "grid-gap" => assign_gap(&mut style.alignment, source, media),
        "place-content" => assign_place_content(&mut style.alignment, source),
        "place-items" => assign_place_items(&mut style.alignment, source),
        "place-self" => assign_place_self(&mut style.alignment, source),
        _ => return false,
    }
    true
}

fn assign<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

pub(super) fn parse_gap(source: &str, media: MediaContext, axis: LengthAxis) -> Option<CssGap> {
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
        return percentage_value(unit_value).map(CssGap::Percent);
    }
    parser.reset(&state);
    parse_length_token(parser).map(|length| CssGap::Cells(media.resolve_cells(length, axis)))
}

fn assign_gap(style: &mut AlignmentStyle, source: &str, media: MediaContext) {
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

/// `physical` admits the inline-axis-only `left` / `right` keywords, which `justify-items` and
/// `justify-self` accept and `align-items` / `align-self` do not.
fn parse_item_alignment(source: &str, physical: bool) -> Option<Alignment<ItemAlignment>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_item_alignment_parser(&mut parser, physical)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_item_alignment_parser(
    parser: &mut Parser<'_, '_>,
    physical: bool,
) -> Option<Alignment<ItemAlignment>> {
    let first = parser.expect_ident_cloned().ok()?.to_ascii_lowercase();
    // `justify-items: legacy left` and friends: the legacy prefix only changes how the value is
    // inherited by table cells, which we do not model, so it is consumed and the keyword stands.
    let first = if physical && first == "legacy" {
        parser.expect_ident_cloned().ok()?.to_ascii_lowercase()
    } else {
        first
    };
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
        "normal" => ItemAlignment::Normal,
        "stretch" if safety == AlignmentSafety::Unsafe => ItemAlignment::Stretch,
        "start" => ItemAlignment::Start,
        "end" => ItemAlignment::End,
        "left" if physical => ItemAlignment::Start,
        "right" if physical => ItemAlignment::End,
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

fn parse_self_alignment(source: &str, physical: bool) -> Option<Option<Alignment<ItemAlignment>>> {
    if parse_ident(source).as_deref() == Some("auto") {
        Some(None)
    } else {
        parse_item_alignment(source, physical).map(Some)
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

fn assign_place_content(style: &mut AlignmentStyle, source: &str) {
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

fn assign_place_items(style: &mut AlignmentStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let Some(align) = parse_item_alignment_parser(&mut parser, false) else {
        return;
    };
    let justify = if parser.is_exhausted() {
        align
    } else {
        let Some(value) = parse_item_alignment_parser(&mut parser, true) else {
            return;
        };
        value
    };
    if parser.expect_exhausted().is_err() {
        return;
    }
    style.align_items = align;
    style.justify_items = justify;
}

/// `place-self` differs from `place-items` in accepting `auto`, which means "no self value".
fn assign_place_self(style: &mut AlignmentStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let Some(align) = parse_self_alignment_parser(&mut parser, false) else {
        return;
    };
    let justify = if parser.is_exhausted() {
        align
    } else {
        let Some(value) = parse_self_alignment_parser(&mut parser, true) else {
            return;
        };
        value
    };
    if parser.expect_exhausted().is_err() {
        return;
    }
    style.align_self = align;
    style.justify_self = justify;
}

fn parse_self_alignment_parser(
    parser: &mut Parser<'_, '_>,
    physical: bool,
) -> Option<Option<Alignment<ItemAlignment>>> {
    let state = parser.state();
    if parser
        .try_parse(|input| input.expect_ident_matching("auto"))
        .is_ok()
    {
        return Some(None);
    }
    parser.reset(&state);
    parse_item_alignment_parser(parser, physical).map(Some)
}
