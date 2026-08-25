use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use super::flex::apply_flex_declaration;
use crate::core::style::{
    BorderCollapse, BorderSpacing, BoxSizing, CaptionSide, ComputedStyle, CssInset, CssMargin,
    CssMaxSize, CssPercentage, CssSize, InsetEdges, LegacyAlign, LengthAxis, ListStyleType,
    MarginEdges, Overflow, Position, TableLayoutMode, TextAlign, VerticalAlign, Visibility,
    WhiteSpace,
};
use crate::css::Declaration;
use crate::css::values::{
    assign_border_color, assign_border_colors, assign_border_side, assign_border_style,
    assign_border_styles, assign_border_width, assign_border_widths, assign_edges, assign_one,
    consume_block, parse_background_color, parse_border, parse_color, parse_cursor, parse_display,
    parse_font_weight, parse_ident, parse_length_token, parse_lengths, parse_list_style_position,
    parse_list_style_type, parse_size, parse_text_decoration,
};

pub(super) fn apply_declaration(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    declaration: &Declaration,
    media: MediaContext,
) {
    if declaration.name == "font-size" {
        return;
    }
    let (font_px, root_font_px) = media.layout_font_sizes();
    if let Some(keyword) = parse_ident(&declaration.value)
        && matches!(keyword.as_str(), "initial" | "inherit" | "unset")
    {
        apply_css_wide(style, parent_style, &declaration.name, &keyword);
        return;
    }
    if apply_flex_declaration(style, &declaration.name, &declaration.value, media) {
        return;
    }
    match declaration.name.as_str() {
        "display" => {
            if let Some(display) = parse_display(&declaration.value) {
                style.display = display;
            }
        }
        "overflow" => {
            if let Some((x, y)) = parse_overflow(&declaration.value) {
                style.overflow.x = x;
                style.overflow.y = y;
            }
        }
        "overflow-x" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| Overflow::parse(&value))
            {
                style.overflow.x = value;
            }
        }
        "overflow-y" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| Overflow::parse(&value))
            {
                style.overflow.y = value;
            }
        }
        "visibility" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| Visibility::parse(&value))
            {
                style.visibility = value;
            }
        }
        "position" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| Position::parse(&value))
            {
                style.position = value;
            }
        }
        "inset" => {
            if let Some(value) = parse_insets(&declaration.value, media) {
                style.inset = value;
            }
        }
        "top" => assign_inset(
            &mut style.inset.top,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "right" => assign_inset(
            &mut style.inset.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "bottom" => assign_inset(
            &mut style.inset.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "left" => assign_inset(
            &mut style.inset.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "white-space" => {
            if let Some(white_space) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "normal" => Some(WhiteSpace::Normal),
                    "nowrap" => Some(WhiteSpace::NoWrap),
                    "pre" => Some(WhiteSpace::Pre),
                    "pre-wrap" => Some(WhiteSpace::PreWrap),
                    "pre-line" => Some(WhiteSpace::PreLine),
                    "break-spaces" => Some(WhiteSpace::BreakSpaces),
                    _ => None,
                })
            {
                style.white_space = white_space;
            }
        }
        "cursor" => {
            if let Some(cursor) = parse_cursor(&declaration.value) {
                style.cursor = cursor;
            }
        }
        "text-align" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "start" => Some(TextAlign::Start),
                    "left" => Some(TextAlign::Left),
                    "right" => Some(TextAlign::Right),
                    "center" => Some(TextAlign::Center),
                    "justify" => Some(TextAlign::Justify),
                    _ => None,
                })
            {
                style.text_align = value;
                style.legacy_align = LegacyAlign::None;
            }
        }
        "-textsurfer-legacy-align" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "left" => Some(LegacyAlign::Left),
                    "right" => Some(LegacyAlign::Right),
                    "center" => Some(LegacyAlign::Center),
                    _ => None,
                })
            {
                style.legacy_align = value;
            }
        }
        "vertical-align" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "baseline" => Some(VerticalAlign::Baseline),
                    "top" => Some(VerticalAlign::Top),
                    "middle" => Some(VerticalAlign::Middle),
                    "bottom" => Some(VerticalAlign::Bottom),
                    _ => None,
                })
            {
                style.vertical_align = value;
            }
        }
        "width" => {
            if let Some(width) = parse_size(
                &declaration.value,
                media.cell_metric,
                media.viewport,
                font_px,
                root_font_px,
                LengthAxis::Horizontal,
            ) {
                style.width = width;
            }
        }
        "height" => assign_size(
            &mut style.height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "min-width" => assign_size(
            &mut style.min_width,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "min-height" => assign_size(
            &mut style.min_height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "max-width" => assign_max_size(
            &mut style.max_width,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "max-height" => assign_max_size(
            &mut style.max_height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "box-sizing" => {
            if let Some(box_sizing) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "content-box" => Some(BoxSizing::ContentBox),
                    "border-box" => Some(BoxSizing::BorderBox),
                    _ => None,
                })
            {
                style.box_sizing = box_sizing;
            }
        }
        "margin" => {
            if let Some(values) = parse_margins(&declaration.value, media) {
                style.margin = values;
            }
        }
        "padding" => {
            if let Some(values) = parse_lengths(&declaration.value) {
                assign_edges(
                    &mut style.padding,
                    &values,
                    media.cell_metric,
                    media.viewport,
                    font_px,
                    root_font_px,
                );
            }
        }
        "margin-top" => assign_margin(
            &mut style.margin.top,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "margin-right" => assign_margin(
            &mut style.margin.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "margin-bottom" => assign_margin(
            &mut style.margin.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media,
        ),
        "margin-left" => assign_margin(
            &mut style.margin.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
        ),
        "padding-top" => assign_one(
            &mut style.padding.top,
            &declaration.value,
            LengthAxis::Vertical,
            media.cell_metric,
            media.viewport,
            font_px,
            root_font_px,
        ),
        "padding-right" => assign_one(
            &mut style.padding.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media.cell_metric,
            media.viewport,
            font_px,
            root_font_px,
        ),
        "padding-bottom" => assign_one(
            &mut style.padding.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media.cell_metric,
            media.viewport,
            font_px,
            root_font_px,
        ),
        "padding-left" => assign_one(
            &mut style.padding.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media.cell_metric,
            media.viewport,
            font_px,
            root_font_px,
        ),
        "border" => {
            if let Some(border) = parse_border(&declaration.value) {
                style.border = border;
            }
        }
        "border-top" => assign_border_side(&mut style.border.top, &declaration.value),
        "border-right" => assign_border_side(&mut style.border.right, &declaration.value),
        "border-bottom" => assign_border_side(&mut style.border.bottom, &declaration.value),
        "border-left" => assign_border_side(&mut style.border.left, &declaration.value),
        "color" => {
            if let Some(color) = parse_color(&declaration.value) {
                style.color = Some(color);
            }
        }
        "background-color" | "background" => {
            if let Some(color) = parse_background_color(&declaration.value) {
                style.background = color;
            }
        }
        "font-weight" => {
            if let Some(bold) = parse_font_weight(&declaration.value) {
                style.bold = bold;
            }
        }
        "text-decoration" | "text-decoration-line" => {
            if let Some((underline, strike)) = parse_text_decoration(&declaration.value) {
                style.underline = underline;
                style.strike = strike;
            }
        }
        "border-style" => assign_border_styles(&mut style.border, &declaration.value),
        "border-top-style" => assign_border_style(&mut style.border.top, &declaration.value),
        "border-right-style" => assign_border_style(&mut style.border.right, &declaration.value),
        "border-bottom-style" => assign_border_style(&mut style.border.bottom, &declaration.value),
        "border-left-style" => assign_border_style(&mut style.border.left, &declaration.value),
        "border-width" => assign_border_widths(&mut style.border, &declaration.value),
        "border-top-width" => assign_border_width(&mut style.border.top, &declaration.value),
        "border-right-width" => assign_border_width(&mut style.border.right, &declaration.value),
        "border-bottom-width" => assign_border_width(&mut style.border.bottom, &declaration.value),
        "border-left-width" => assign_border_width(&mut style.border.left, &declaration.value),
        "border-color" => assign_border_colors(&mut style.border, &declaration.value),
        "border-top-color" => assign_border_color(&mut style.border.top, &declaration.value),
        "border-right-color" => assign_border_color(&mut style.border.right, &declaration.value),
        "border-bottom-color" => assign_border_color(&mut style.border.bottom, &declaration.value),
        "border-left-color" => assign_border_color(&mut style.border.left, &declaration.value),
        "table-layout" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "auto" => Some(TableLayoutMode::Auto),
                    "fixed" => Some(TableLayoutMode::Fixed),
                    _ => None,
                })
            {
                style.table_layout = value;
            }
        }
        "border-collapse" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "separate" => Some(BorderCollapse::Separate),
                    "collapse" => Some(BorderCollapse::Collapse),
                    _ => None,
                })
            {
                style.border_collapse = value;
            }
        }
        "border-spacing" => {
            if let Some(values) = parse_lengths(&declaration.value)
                && let [horizontal] | [horizontal, _] = values.as_slice()
            {
                style.border_spacing = BorderSpacing::new(
                    media.resolve_cells(*horizontal, LengthAxis::Horizontal),
                    media.resolve_cells(
                        values.get(1).copied().unwrap_or(*horizontal),
                        LengthAxis::Vertical,
                    ),
                );
            }
        }
        "caption-side" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "top" => Some(CaptionSide::Top),
                    "bottom" => Some(CaptionSide::Bottom),
                    _ => None,
                })
            {
                style.caption_side = value;
            }
        }
        "list-style-type" => {
            if let Some(value) = parse_ident(&declaration.value)
                .as_deref()
                .and_then(parse_list_style_type)
            {
                style.list_style_type = value;
            }
        }
        "list-style-position" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| parse_list_style_position(&value))
            {
                style.list_style_position = value;
            }
        }
        "list-style" => apply_list_style_shorthand(style, &declaration.value),
        _ => {}
    }
}

fn apply_css_wide(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    property: &str,
    keyword: &str,
) {
    let initial = ComputedStyle::default();
    let inherited = parent_style.unwrap_or_default();
    let source = if keyword == "inherit" || (keyword == "unset" && is_inherited(property)) {
        inherited
    } else {
        initial
    };
    match property {
        "display" => style.display = source.display,
        "overflow" => style.overflow = source.overflow,
        "overflow-x" => style.overflow.x = source.overflow.x,
        "overflow-y" => style.overflow.y = source.overflow.y,
        "visibility" => style.visibility = source.visibility,
        "position" => style.position = source.position,
        "inset" => style.inset = source.inset,
        "top" => style.inset.top = source.inset.top,
        "right" => style.inset.right = source.inset.right,
        "bottom" => style.inset.bottom = source.inset.bottom,
        "left" => style.inset.left = source.inset.left,
        "white-space" => style.white_space = source.white_space,
        "cursor" => style.cursor = source.cursor,
        "text-align" => {
            style.text_align = source.text_align;
            style.legacy_align = source.legacy_align;
        }
        "vertical-align" => style.vertical_align = source.vertical_align,
        "color" => style.color = source.color,
        "background-color" | "background" => style.background = source.background,
        "font-weight" => style.bold = source.bold,
        "text-decoration" | "text-decoration-line" => {
            style.underline = source.underline;
            style.strike = source.strike;
        }
        "width" => style.width = source.width,
        "height" => style.height = source.height,
        "min-width" => style.min_width = source.min_width,
        "min-height" => style.min_height = source.min_height,
        "max-width" => style.max_width = source.max_width,
        "max-height" => style.max_height = source.max_height,
        "flex-direction" => style.flex.direction = source.flex.direction,
        "flex-wrap" => style.flex.wrap = source.flex.wrap,
        "flex-flow" => {
            style.flex.direction = source.flex.direction;
            style.flex.wrap = source.flex.wrap;
        }
        "flex-grow" => style.flex.grow = source.flex.grow,
        "flex-shrink" => style.flex.shrink = source.flex.shrink,
        "flex-basis" => style.flex.basis = source.flex.basis,
        "flex" => {
            style.flex.grow = source.flex.grow;
            style.flex.shrink = source.flex.shrink;
            style.flex.basis = source.flex.basis;
        }
        "order" => style.flex.order = source.flex.order,
        "justify-content" => style.flex.justify_content = source.flex.justify_content,
        "align-items" => style.flex.align_items = source.flex.align_items,
        "align-self" => style.flex.align_self = source.flex.align_self,
        "align-content" => style.flex.align_content = source.flex.align_content,
        "row-gap" => style.flex.row_gap = source.flex.row_gap,
        "column-gap" => style.flex.column_gap = source.flex.column_gap,
        "gap" => {
            style.flex.row_gap = source.flex.row_gap;
            style.flex.column_gap = source.flex.column_gap;
        }
        "place-content" => {
            style.flex.align_content = source.flex.align_content;
            style.flex.justify_content = source.flex.justify_content;
        }
        "box-sizing" => style.box_sizing = source.box_sizing,
        "margin" => style.margin = source.margin,
        "padding" => style.padding = source.padding,
        "margin-top" => style.margin.top = source.margin.top,
        "margin-right" => style.margin.right = source.margin.right,
        "margin-bottom" => style.margin.bottom = source.margin.bottom,
        "margin-left" => style.margin.left = source.margin.left,
        "padding-top" => style.padding.top = source.padding.top,
        "padding-right" => style.padding.right = source.padding.right,
        "padding-bottom" => style.padding.bottom = source.padding.bottom,
        "padding-left" => style.padding.left = source.padding.left,
        "border" | "border-style" | "border-width" | "border-color" => style.border = source.border,
        "border-top" | "border-top-style" | "border-top-width" | "border-top-color" => {
            style.border.top = source.border.top
        }
        "border-right" | "border-right-style" | "border-right-width" | "border-right-color" => {
            style.border.right = source.border.right
        }
        "border-bottom" | "border-bottom-style" | "border-bottom-width" | "border-bottom-color" => {
            style.border.bottom = source.border.bottom
        }
        "border-left" | "border-left-style" | "border-left-width" | "border-left-color" => {
            style.border.left = source.border.left
        }
        "table-layout" => style.table_layout = source.table_layout,
        "border-collapse" => style.border_collapse = source.border_collapse,
        "border-spacing" => style.border_spacing = source.border_spacing,
        "caption-side" => style.caption_side = source.caption_side,
        "list-style-type" => style.list_style_type = source.list_style_type,
        "list-style-position" => style.list_style_position = source.list_style_position,
        "list-style" => {
            style.list_style_type = source.list_style_type;
            style.list_style_position = source.list_style_position;
        }
        _ => {}
    }
}

fn is_inherited(property: &str) -> bool {
    matches!(
        property,
        "white-space"
            | "cursor"
            | "visibility"
            | "text-align"
            | "color"
            | "font-weight"
            | "border-collapse"
            | "border-spacing"
            | "caption-side"
            | "list-style"
            | "list-style-type"
            | "list-style-position"
    )
}

fn parse_overflow(source: &str) -> Option<(Overflow, Overflow)> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let first = Overflow::parse(parser.expect_ident().ok()?.as_ref())?;
    let second = if parser.is_exhausted() {
        first
    } else {
        Overflow::parse(parser.expect_ident().ok()?.as_ref())?
    };
    parser.expect_exhausted().ok()?;
    Some((first, second))
}

#[derive(Clone, Copy)]
enum ParsedInset {
    Auto,
    Length(crate::core::style::CssLength),
    Percent(CssPercentage),
}

fn parse_insets(source: &str, media: MediaContext) -> Option<InsetEdges> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() {
        if values.len() == 4 {
            return None;
        }
        values.push(parse_inset_token(&mut parser)?);
    }
    let [top, right, bottom, left] = match values.as_slice() {
        [all] => [*all; 4],
        [vertical, horizontal] => [*vertical, *horizontal, *vertical, *horizontal],
        [top, horizontal, bottom] => [*top, *horizontal, *bottom, *horizontal],
        [top, right, bottom, left] => [*top, *right, *bottom, *left],
        _ => return None,
    };
    Some(InsetEdges {
        top: resolve_inset(top, LengthAxis::Vertical, media),
        right: resolve_inset(right, LengthAxis::Horizontal, media),
        bottom: resolve_inset(bottom, LengthAxis::Vertical, media),
        left: resolve_inset(left, LengthAxis::Horizontal, media),
    })
}

fn assign_inset(target: &mut CssInset, source: &str, axis: LengthAxis, media: MediaContext) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    if let Some(value) = parse_inset_token(&mut parser)
        && parser.expect_exhausted().is_ok()
    {
        *target = resolve_inset(value, axis, media);
    }
}

fn parse_inset_token(parser: &mut Parser<'_, '_>) -> Option<ParsedInset> {
    if parser
        .try_parse(|input| input.expect_ident_matching("auto"))
        .is_ok()
    {
        return Some(ParsedInset::Auto);
    }
    if let Ok(unit_value) = parser.try_parse(|input| {
        let token = input.next()?.clone();
        match token {
            Token::Percentage { unit_value, .. } if unit_value.is_finite() && unit_value >= 0.0 => {
                Ok(unit_value)
            }
            token => Err(input.new_unexpected_token_error::<()>(token)),
        }
    }) {
        return Some(ParsedInset::Percent(CssPercentage::new(
            (unit_value * 10_000.0).round().min(u32::MAX as f32) as u32,
        )));
    }
    Some(ParsedInset::Length(
        crate::css::values::parse_signed_length_token(parser)?,
    ))
}

fn resolve_inset(value: ParsedInset, axis: LengthAxis, media: MediaContext) -> CssInset {
    match value {
        ParsedInset::Auto => CssInset::Auto,
        ParsedInset::Length(length) => CssInset::Cells(media.resolve_signed_cells(length, axis)),
        ParsedInset::Percent(value) => CssInset::Percent(value),
    }
}

fn parse_margins(source: &str, media: MediaContext) -> Option<MarginEdges> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        let value = if parser
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
        {
            CssMargin::Auto
        } else {
            let length = parse_length_token(&mut parser)?;
            let axis = if matches!(values.len(), 0 | 2) {
                LengthAxis::Vertical
            } else {
                LengthAxis::Horizontal
            };
            CssMargin::Cells(media.resolve_cells(length, axis))
        };
        values.push(value);
    }
    if values.is_empty() || !parser.is_exhausted() {
        return None;
    }
    let (top, right, bottom, left) = match values.as_slice() {
        [all] => (*all, *all, *all, *all),
        [vertical, horizontal] => (*vertical, *horizontal, *vertical, *horizontal),
        [top, horizontal, bottom] => (*top, *horizontal, *bottom, *horizontal),
        [top, right, bottom, left] => (*top, *right, *bottom, *left),
        _ => return None,
    };
    Some(MarginEdges {
        top,
        right,
        bottom,
        left,
    })
}

fn assign_margin(target: &mut CssMargin, source: &str, axis: LengthAxis, media: MediaContext) {
    if parse_ident(source).as_deref() == Some("auto") {
        *target = CssMargin::Auto;
        return;
    }
    let Some(length) = parse_lengths(source).and_then(|values| match values.as_slice() {
        [value] => Some(*value),
        _ => None,
    }) else {
        return;
    };
    *target = CssMargin::Cells(media.resolve_cells(length, axis));
}

fn assign_size(target: &mut CssSize, source: &str, axis: LengthAxis, media: MediaContext) {
    let (font_px, root_font_px) = media.layout_font_sizes();
    if let Some(value) = parse_size(
        source,
        media.cell_metric,
        media.viewport,
        font_px,
        root_font_px,
        axis,
    ) {
        *target = value;
    }
}

fn assign_max_size(target: &mut CssMaxSize, source: &str, axis: LengthAxis, media: MediaContext) {
    if parse_ident(source).as_deref() == Some("none") {
        *target = CssMaxSize::None;
        return;
    }
    let mut value = CssSize::Auto;
    assign_size(&mut value, source, axis, media);
    *target = match value {
        CssSize::Cells(value) => CssMaxSize::Cells(value),
        CssSize::Percent(value) => CssMaxSize::Percent(value),
        CssSize::Auto => return,
    };
}

/// `list-style` sets type, position and image; `none` may stand for either type or image, and an
/// image we cannot render leaves the type alone.
fn apply_list_style_shorthand(style: &mut ComputedStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut list_type = None;
    let mut position = None;
    let mut saw_none = false;
    while !parser.is_exhausted() {
        let Ok(token) = parser.next().cloned() else {
            return;
        };
        match token {
            Token::Ident(value) => {
                if value.eq_ignore_ascii_case("none") {
                    saw_none = true;
                } else if let Some(value) = parse_list_style_position(&value) {
                    position = Some(value);
                } else if let Some(value) = parse_list_style_type(&value) {
                    list_type = Some(value);
                } else {
                    return;
                }
            }
            Token::UnquotedUrl(_) => {}
            Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                if parser.parse_nested_block(consume_block).is_err() {
                    return;
                }
            }
            _ => return,
        }
    }
    if let Some(value) = list_type {
        style.list_style_type = value;
    } else if saw_none {
        style.list_style_type = ListStyleType::None;
    }
    if let Some(value) = position {
        style.list_style_position = value;
    }
}
