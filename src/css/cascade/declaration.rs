use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use crate::core::style::{
    BorderCollapse, BorderSpacing, BoxSizing, CaptionSide, ComputedStyle, CssMargin, LegacyAlign,
    LengthAxis, ListStyleType, MarginEdges, TableLayoutMode, TextAlign, VerticalAlign, WhiteSpace,
};
use crate::css::Declaration;
use crate::css::values::{
    assign_border_color, assign_border_colors, assign_border_side, assign_border_style,
    assign_border_styles, assign_border_width, assign_border_widths, assign_edges, assign_one,
    consume_block, parse_background_color, parse_border, parse_color, parse_display,
    parse_font_weight, parse_ident, parse_length_token, parse_lengths, parse_list_style_position,
    parse_list_style_type, parse_text_decoration, parse_width,
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
    match declaration.name.as_str() {
        "display" => {
            if let Some(display) = parse_display(&declaration.value) {
                style.display = display;
            }
        }
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
            if let Some(width) = parse_width(
                &declaration.value,
                media.cell_metric,
                media.viewport,
                font_px,
                root_font_px,
            ) {
                style.width = width;
            }
        }
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
        "white-space" => style.white_space = source.white_space,
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
