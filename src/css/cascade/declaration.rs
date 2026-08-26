use cssparser::{Parser, ParserInput, Token};

use super::MediaContext;
use super::alignment::apply_alignment_declaration;
use super::flex::apply_flex_declaration;
use super::grid::apply_grid_declaration;
use crate::core::style::{
    BorderCollapse, BorderSpacing, BoxSizing, CalcRange, CaptionSide, ComputedStyle, CssInset,
    CssMargin, CssMaxSize, CssPadding, CssPercentage, CssSignedPercentage, CssSize, InsetEdges,
    LegacyAlign, LengthAxis, ListStyleType, MarginEdges, Overflow, PaddingEdges, Position,
    StyleStore, TableLayoutMode, TextAlign, VerticalAlign, Visibility, WhiteSpace,
};
use crate::css::Declaration;
use crate::css::values::{
    assign_border_color, assign_border_colors, assign_border_side, assign_border_style,
    assign_border_styles, assign_border_width, assign_border_widths, consume_block,
    parse_background_color, parse_border, parse_color, parse_cursor, parse_display,
    parse_font_weight, parse_ident, parse_lengths, parse_list_style_position,
    parse_list_style_type, parse_opacity, parse_size, parse_text_decoration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ApplyOutcome {
    Applied,
    Invalid,
    Unsupported,
}

pub(super) fn apply_declaration(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    declaration: &Declaration,
    media: MediaContext,
    store: &mut StyleStore,
) -> ApplyOutcome {
    if declaration.name == "font-size" || !is_supported_property(&declaration.name) {
        return ApplyOutcome::Unsupported;
    }
    let before = *style;
    apply_declaration_raw(style, parent_style, ua_style, declaration, media, store);
    if *style != before || declaration_value_is_valid(parent_style, ua_style, declaration, media) {
        ApplyOutcome::Applied
    } else {
        ApplyOutcome::Invalid
    }
}

fn apply_declaration_raw(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    declaration: &Declaration,
    media: MediaContext,
    store: &mut StyleStore,
) {
    if declaration.name == "font-size" {
        return;
    }
    if let Some(keyword) = parse_ident(&declaration.value)
        && matches!(keyword.as_str(), "initial" | "inherit" | "unset" | "revert")
    {
        apply_css_wide(style, parent_style, ua_style, &declaration.name, &keyword);
        return;
    }
    if apply_flex_declaration(style, &declaration.name, &declaration.value, media, store)
        || apply_alignment_declaration(style, &declaration.name, &declaration.value, media, store)
        || apply_grid_declaration(style, &declaration.name, &declaration.value, media, store)
    {
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
        // Only fully transparent is honoured, and it becomes `visibility: hidden` — the two have
        // identical layout behaviour, geometry retained and nothing painted, so this rides
        // machinery that already exists end to end. Partial alpha over arbitrary content stays
        // unimplemented and renders opaque.
        //
        // It earns its place because `opacity: 0` is *the* way the web builds a custom control:
        // a real `<input>` laid invisibly over a styled box. Wikipedia's header has three, and
        // without this they paint as stray checkboxes over the article chrome.
        "opacity" => {
            if let Some(value) = parse_opacity(&declaration.value)
                && value == 0.0
            {
                style.visibility = Visibility::Hidden;
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
            if let Some(value) = parse_insets(&declaration.value, media, store) {
                style.inset = value;
            }
        }
        "top" => assign_inset(
            &mut style.inset.top,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "right" => assign_inset(
            &mut style.inset.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "bottom" => assign_inset(
            &mut style.inset.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "left" => assign_inset(
            &mut style.inset.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
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
        "width" => assign_size(
            &mut style.width,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "height" => assign_size(
            &mut style.height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "min-width" => assign_size(
            &mut style.min_width,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "min-height" => assign_size(
            &mut style.min_height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "max-width" => assign_max_size(
            &mut style.max_width,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "max-height" => assign_max_size(
            &mut style.max_height,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
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
            if let Some(values) = parse_margins(&declaration.value, media, store) {
                style.margin = values;
            }
        }
        "padding" => {
            if let Some(values) = parse_paddings(&declaration.value, media, store) {
                style.padding = values;
            }
        }
        "margin-top" => assign_margin(
            &mut style.margin.top,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "margin-right" => assign_margin(
            &mut style.margin.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "margin-bottom" => assign_margin(
            &mut style.margin.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "margin-left" => assign_margin(
            &mut style.margin.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "padding-top" => assign_padding(
            &mut style.padding.top,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "padding-right" => assign_padding(
            &mut style.padding.right,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
        ),
        "padding-bottom" => assign_padding(
            &mut style.padding.bottom,
            &declaration.value,
            LengthAxis::Vertical,
            media,
            store,
        ),
        "padding-left" => assign_padding(
            &mut style.padding.left,
            &declaration.value,
            LengthAxis::Horizontal,
            media,
            store,
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

pub(super) fn declaration_value_is_valid(
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    declaration: &Declaration,
    media: MediaContext,
) -> bool {
    if parse_ident(&declaration.value).is_some_and(|keyword| {
        matches!(keyword.as_str(), "initial" | "inherit" | "unset" | "revert")
    }) {
        return true;
    }
    let mut probes = [
        ComputedStyle::default(),
        ua_style,
        parent_style.unwrap_or_default(),
    ];
    probes.iter_mut().any(|probe| {
        let before = *probe;
        let mut store = StyleStore::default();
        apply_declaration_raw(
            probe,
            parent_style,
            ua_style,
            declaration,
            media,
            &mut store,
        );
        *probe != before
    })
}

fn is_supported_property(property: &str) -> bool {
    matches!(
        property,
        "display"
            | "overflow"
            | "overflow-x"
            | "overflow-y"
            | "visibility"
            | "opacity"
            | "position"
            | "inset"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "white-space"
            | "cursor"
            | "text-align"
            | "-textsurfer-legacy-align"
            | "vertical-align"
            | "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "box-sizing"
            | "margin"
            | "padding"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "color"
            | "background-color"
            | "background"
            | "font-weight"
            | "text-decoration"
            | "text-decoration-line"
            | "border-style"
            | "border-top-style"
            | "border-right-style"
            | "border-bottom-style"
            | "border-left-style"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "border-color"
            | "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color"
            | "table-layout"
            | "border-collapse"
            | "border-spacing"
            | "caption-side"
            | "list-style-type"
            | "list-style-position"
            | "list-style"
            | "flex-direction"
            | "flex-wrap"
            | "flex-flow"
            | "flex-grow"
            | "flex-shrink"
            | "flex-basis"
            | "flex"
            | "order"
            | "justify-content"
            | "align-content"
            | "justify-items"
            | "align-items"
            | "justify-self"
            | "align-self"
            | "row-gap"
            | "column-gap"
            | "gap"
            | "grid-row-gap"
            | "grid-column-gap"
            | "grid-gap"
            | "place-content"
            | "place-items"
            | "place-self"
            | "grid-template-columns"
            | "grid-template-rows"
            | "grid-template-areas"
            | "grid-template"
            | "grid-auto-columns"
            | "grid-auto-rows"
            | "grid-auto-flow"
            | "grid"
            | "grid-row-start"
            | "grid-row-end"
            | "grid-column-start"
            | "grid-column-end"
            | "grid-row"
            | "grid-column"
            | "grid-area"
    )
}

fn apply_css_wide(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    property: &str,
    keyword: &str,
) {
    let initial = ComputedStyle::default();
    let inherited = parent_style.unwrap_or_default();
    let source = if keyword == "revert" {
        ua_style
    } else if keyword == "inherit" || (keyword == "unset" && is_inherited(property)) {
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
        "-textsurfer-legacy-align" => style.legacy_align = source.legacy_align,
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
        "order" => style.order = source.order,
        "justify-content" => style.alignment.justify_content = source.alignment.justify_content,
        "align-content" => style.alignment.align_content = source.alignment.align_content,
        "justify-items" => style.alignment.justify_items = source.alignment.justify_items,
        "align-items" => style.alignment.align_items = source.alignment.align_items,
        "justify-self" => style.alignment.justify_self = source.alignment.justify_self,
        "align-self" => style.alignment.align_self = source.alignment.align_self,
        "row-gap" | "grid-row-gap" => style.alignment.row_gap = source.alignment.row_gap,
        "column-gap" | "grid-column-gap" => {
            style.alignment.column_gap = source.alignment.column_gap
        }
        "gap" | "grid-gap" => {
            style.alignment.row_gap = source.alignment.row_gap;
            style.alignment.column_gap = source.alignment.column_gap;
        }
        "place-content" => {
            style.alignment.align_content = source.alignment.align_content;
            style.alignment.justify_content = source.alignment.justify_content;
        }
        "place-items" => {
            style.alignment.align_items = source.alignment.align_items;
            style.alignment.justify_items = source.alignment.justify_items;
        }
        "place-self" => {
            style.alignment.align_self = source.alignment.align_self;
            style.alignment.justify_self = source.alignment.justify_self;
        }
        "grid-template-columns" => style.grid.template_columns = source.grid.template_columns,
        "grid-template-rows" => style.grid.template_rows = source.grid.template_rows,
        "grid-template-areas" => style.grid.template_areas = source.grid.template_areas,
        "grid-template" => {
            style.grid.template_columns = source.grid.template_columns;
            style.grid.template_rows = source.grid.template_rows;
            style.grid.template_areas = source.grid.template_areas;
        }
        "grid-auto-columns" => style.grid.auto_columns = source.grid.auto_columns,
        "grid-auto-rows" => style.grid.auto_rows = source.grid.auto_rows,
        "grid-auto-flow" => style.grid.auto_flow = source.grid.auto_flow,
        "grid" => {
            style.grid.template_columns = source.grid.template_columns;
            style.grid.template_rows = source.grid.template_rows;
            style.grid.template_areas = source.grid.template_areas;
            style.grid.auto_columns = source.grid.auto_columns;
            style.grid.auto_rows = source.grid.auto_rows;
            style.grid.auto_flow = source.grid.auto_flow;
        }
        "grid-row-start" => style.grid.row.start = source.grid.row.start,
        "grid-row-end" => style.grid.row.end = source.grid.row.end,
        "grid-column-start" => style.grid.column.start = source.grid.column.start,
        "grid-column-end" => style.grid.column.end = source.grid.column.end,
        "grid-row" => style.grid.row = source.grid.row,
        "grid-column" => style.grid.column = source.grid.column,
        "grid-area" => {
            style.grid.row = source.grid.row;
            style.grid.column = source.grid.column;
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

#[derive(Clone)]
enum ParsedEdge {
    Auto,
    Length(crate::core::style::CssLength),
    Percent(f32),
    Math(crate::css::math::ParsedMath),
}

fn parse_insets(source: &str, media: MediaContext, store: &mut StyleStore) -> Option<InsetEdges> {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() {
        if values.len() == 4 {
            return None;
        }
        values.push(parse_edge_token(&mut parser, true)?);
    }
    let [top, right, bottom, left] = match values.as_slice() {
        [all] => [all.clone(), all.clone(), all.clone(), all.clone()],
        [vertical, horizontal] => [
            vertical.clone(),
            horizontal.clone(),
            vertical.clone(),
            horizontal.clone(),
        ],
        [top, horizontal, bottom] => [
            top.clone(),
            horizontal.clone(),
            bottom.clone(),
            horizontal.clone(),
        ],
        [top, right, bottom, left] => [top.clone(), right.clone(), bottom.clone(), left.clone()],
        _ => return None,
    };
    let result = (|| {
        Some(InsetEdges {
            top: resolve_inset(top, LengthAxis::Vertical, media, store)?,
            right: resolve_inset(right, LengthAxis::Horizontal, media, store)?,
            bottom: resolve_inset(bottom, LengthAxis::Vertical, media, store)?,
            left: resolve_inset(left, LengthAxis::Horizontal, media, store)?,
        })
    })();
    if result.is_none() {
        store.rollback(checkpoint);
    }
    result
}

fn assign_inset(
    target: &mut CssInset,
    source: &str,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    if let Some(value) = parse_edge_token(&mut parser, true)
        && parser.expect_exhausted().is_ok()
        && let Some(value) = resolve_inset(value, axis, media, store)
    {
        *target = value;
    } else {
        store.rollback(checkpoint);
    }
}

fn parse_edge_token(parser: &mut Parser<'_, '_>, allow_auto: bool) -> Option<ParsedEdge> {
    if allow_auto
        && parser
            .try_parse(|input| input.expect_ident_matching("auto"))
            .is_ok()
    {
        return Some(ParsedEdge::Auto);
    }
    if let Some(value) = crate::css::math::parse_math_parser(parser) {
        return Some(ParsedEdge::Math(value));
    }
    if let Ok(unit_value) = parser.try_parse(|input| {
        let token = input.next()?.clone();
        match token {
            Token::Percentage { unit_value, .. } if unit_value.is_finite() => Ok(unit_value),
            token => Err(input.new_unexpected_token_error::<()>(token)),
        }
    }) {
        return Some(ParsedEdge::Percent(unit_value));
    }
    Some(ParsedEdge::Length(
        crate::css::values::parse_signed_length_token(parser)?,
    ))
}

fn signed_percentage(value: f32) -> Option<CssSignedPercentage> {
    let basis_points = (value * 10_000.0).round();
    if !basis_points.is_finite() || basis_points < i32::MIN as f32 || basis_points > i32::MAX as f32
    {
        return None;
    }
    Some(CssSignedPercentage::new(basis_points as i32))
}

fn resolve_inset(
    value: ParsedEdge,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<CssInset> {
    Some(match value {
        ParsedEdge::Auto => CssInset::Auto,
        ParsedEdge::Length(length) => CssInset::Cells(media.resolve_signed_cells(length, axis)),
        ParsedEdge::Percent(value) => CssInset::Percent(signed_percentage(value)?),
        ParsedEdge::Math(value) => CssInset::Calc(
            store
                .calculations
                .insert(value.lower_cells(media, axis, axis)?, CalcRange::Unbounded)?,
        ),
    })
}

fn parse_margins(source: &str, media: MediaContext, store: &mut StyleStore) -> Option<MarginEdges> {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(parse_edge_token(&mut parser, true)?);
    }
    if values.is_empty() || !parser.is_exhausted() {
        return None;
    }
    let (top, right, bottom, left) = match values.as_slice() {
        [all] => (all.clone(), all.clone(), all.clone(), all.clone()),
        [vertical, horizontal] => (
            vertical.clone(),
            horizontal.clone(),
            vertical.clone(),
            horizontal.clone(),
        ),
        [top, horizontal, bottom] => (
            top.clone(),
            horizontal.clone(),
            bottom.clone(),
            horizontal.clone(),
        ),
        [top, right, bottom, left] => (top.clone(), right.clone(), bottom.clone(), left.clone()),
        _ => return None,
    };
    let result = (|| {
        Some(MarginEdges {
            top: resolve_margin(top, LengthAxis::Vertical, media, store)?,
            right: resolve_margin(right, LengthAxis::Horizontal, media, store)?,
            bottom: resolve_margin(bottom, LengthAxis::Vertical, media, store)?,
            left: resolve_margin(left, LengthAxis::Horizontal, media, store)?,
        })
    })();
    if result.is_none() {
        store.rollback(checkpoint);
    }
    result
}

fn assign_margin(
    target: &mut CssMargin,
    source: &str,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    if let Some(value) = parse_edge_token(&mut parser, true)
        && parser.expect_exhausted().is_ok()
        && let Some(value) = resolve_margin(value, axis, media, store)
    {
        *target = value;
    } else {
        store.rollback(checkpoint);
    }
}

fn resolve_margin(
    value: ParsedEdge,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<CssMargin> {
    Some(match value {
        ParsedEdge::Auto => CssMargin::Auto,
        ParsedEdge::Length(length) => CssMargin::Cells(media.resolve_signed_cells(length, axis)),
        ParsedEdge::Percent(value) if axis == LengthAxis::Horizontal => {
            CssMargin::Percent(signed_percentage(value)?)
        }
        ParsedEdge::Percent(value) => CssMargin::Calc(store.calculations.insert(
            crate::css::math::ParsedMath::Percent(value).lower_cells(
                media,
                axis,
                LengthAxis::Horizontal,
            )?,
            CalcRange::Unbounded,
        )?),
        ParsedEdge::Math(value) => CssMargin::Calc(store.calculations.insert(
            value.lower_cells(media, axis, LengthAxis::Horizontal)?,
            CalcRange::Unbounded,
        )?),
    })
}

fn parse_paddings(
    source: &str,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<PaddingEdges> {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(parse_edge_token(&mut parser, false)?);
    }
    if values.is_empty() || !parser.is_exhausted() {
        return None;
    }
    let (top, right, bottom, left) = match values.as_slice() {
        [all] => (all.clone(), all.clone(), all.clone(), all.clone()),
        [vertical, horizontal] => (
            vertical.clone(),
            horizontal.clone(),
            vertical.clone(),
            horizontal.clone(),
        ),
        [top, horizontal, bottom] => (
            top.clone(),
            horizontal.clone(),
            bottom.clone(),
            horizontal.clone(),
        ),
        [top, right, bottom, left] => (top.clone(), right.clone(), bottom.clone(), left.clone()),
        _ => return None,
    };
    let result = (|| {
        Some(PaddingEdges {
            top: resolve_padding(top, LengthAxis::Vertical, media, store)?,
            right: resolve_padding(right, LengthAxis::Horizontal, media, store)?,
            bottom: resolve_padding(bottom, LengthAxis::Vertical, media, store)?,
            left: resolve_padding(left, LengthAxis::Horizontal, media, store)?,
        })
    })();
    if result.is_none() {
        store.rollback(checkpoint);
    }
    result
}

fn assign_padding(
    target: &mut CssPadding,
    source: &str,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) {
    let checkpoint = store.checkpoint();
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    if let Some(value) = parse_edge_token(&mut parser, false)
        && parser.expect_exhausted().is_ok()
        && let Some(value) = resolve_padding(value, axis, media, store)
    {
        *target = value;
    } else {
        store.rollback(checkpoint);
    }
}

fn resolve_padding(
    value: ParsedEdge,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<CssPadding> {
    Some(match value {
        ParsedEdge::Auto => return None,
        ParsedEdge::Length(length) => {
            let cells = media.resolve_signed_cells(length, axis);
            if cells < 0 {
                return None;
            }
            CssPadding::Cells(cells as usize)
        }
        ParsedEdge::Percent(value) if value < 0.0 => return None,
        ParsedEdge::Percent(value) if axis == LengthAxis::Horizontal => CssPadding::Percent(
            CssPercentage::new((value * 10_000.0).round().min(u32::MAX as f32) as u32),
        ),
        ParsedEdge::Percent(value) => CssPadding::Calc(store.calculations.insert(
            crate::css::math::ParsedMath::Percent(value).lower_cells(
                media,
                axis,
                LengthAxis::Horizontal,
            )?,
            CalcRange::NonNegative,
        )?),
        ParsedEdge::Math(value) => CssPadding::Calc(store.calculations.insert(
            value.lower_cells(media, axis, LengthAxis::Horizontal)?,
            CalcRange::NonNegative,
        )?),
    })
}

fn assign_size(
    target: &mut CssSize,
    source: &str,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) {
    let source_start = source.trim_start().to_ascii_lowercase();
    let is_math = ["calc(", "min(", "max(", "clamp("]
        .iter()
        .any(|function| source_start.starts_with(function));
    if is_math
        && let Some(value) = crate::css::math::parse_length_percentage(source, media, axis)
        && let Some(value) = store
            .calculations
            .insert(value, crate::core::style::CalcRange::NonNegative)
    {
        *target = CssSize::Calc(value);
        return;
    }
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

fn assign_max_size(
    target: &mut CssMaxSize,
    source: &str,
    axis: LengthAxis,
    media: MediaContext,
    store: &mut StyleStore,
) {
    if parse_ident(source).as_deref() == Some("none") {
        *target = CssMaxSize::None;
        return;
    }
    let mut value = CssSize::Auto;
    assign_size(&mut value, source, axis, media, store);
    *target = match value {
        CssSize::Cells(value) => CssMaxSize::Cells(value),
        CssSize::Percent(value) => CssMaxSize::Percent(value),
        CssSize::Calc(value) => CssMaxSize::Calc(value),
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
