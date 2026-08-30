use style::properties::ComputedValues;
use style::properties::longhands;
use style::values::computed::text::TextDecorationLine;
use style::values::specified::box_::AlignmentBaseline;
use style::values::specified::text::TextAlignKeyword;
use style::values::specified::ui::CursorKind;

use crate::core::style::{
    BorderCollapse, BorderSpacing, CaptionSide, Cursor, FontSize, LengthAxis, ListStylePosition,
    ListStyleType, TableLayoutMode, TextAlign, VerticalAlign, Visibility, WhiteSpace,
};

use super::length::Lengths;

pub(super) fn white_space(values: &ComputedValues) -> WhiteSpace {
    use longhands::text_wrap_mode::computed_value::T as Wrap;
    use longhands::white_space_collapse::computed_value::T as Collapse;

    let wraps = matches!(values.clone_text_wrap_mode(), Wrap::Wrap);
    match values.clone_white_space_collapse() {
        Collapse::Collapse if wraps => WhiteSpace::Normal,
        Collapse::Collapse => WhiteSpace::NoWrap,
        Collapse::Preserve if wraps => WhiteSpace::PreWrap,
        Collapse::Preserve => WhiteSpace::Pre,
        // Neither `preserve-breaks` nor `break-spaces` has a no-wrap counterpart in `WhiteSpace`.
        // The collapse mode is what decides how the text reads, so it wins and `nowrap` is dropped.
        Collapse::PreserveBreaks => WhiteSpace::PreLine,
        Collapse::BreakSpaces => WhiteSpace::BreakSpaces,
    }
}

/// `match-parent` is resolved at computed-value time and never reaches us; the `-moz-`/`-webkit-`
/// keywords are aliases for the physical ones. `end` folds to `right` because writing modes are a
/// locked non-goal and `layout.writing-mode.enabled` is off.
pub(super) fn text_align(values: &ComputedValues) -> TextAlign {
    match values.clone_text_align() {
        TextAlignKeyword::Start => TextAlign::Start,
        TextAlignKeyword::Left | TextAlignKeyword::MozLeft => TextAlign::Left,
        TextAlignKeyword::Right | TextAlignKeyword::End | TextAlignKeyword::MozRight => {
            TextAlign::Right
        }
        TextAlignKeyword::Center | TextAlignKeyword::MozCenter => TextAlign::Center,
        TextAlignKeyword::Justify => TextAlign::Justify,
    }
}

/// Stylo 0.20 has no CSS 2.1 `vertical-align`: the shorthand expands to `alignment-baseline` and
/// friends, and the servo build accepts only these four keywords. `top` and `bottom` do not parse
/// at all — recorded in the M7 divergence ledger.
pub(super) fn vertical_align(values: &ComputedValues) -> VerticalAlign {
    match values.clone_alignment_baseline() {
        AlignmentBaseline::Baseline => VerticalAlign::Baseline,
        AlignmentBaseline::TextTop => VerticalAlign::Top,
        AlignmentBaseline::TextBottom => VerticalAlign::Bottom,
        AlignmentBaseline::Middle => VerticalAlign::Middle,
    }
}

pub(super) fn bold(values: &ComputedValues) -> bool {
    values.clone_font_weight().value() > 500.0
}

/// The element's own decorations. They are propagated to descendants by `super::policy`, because
/// `text-decoration-line` is a reset property in Stylo while our `underline`/`strike` inherit.
pub(super) fn decorations(values: &ComputedValues) -> TextDecorationLine {
    values.clone_text_decoration_line()
}

pub(super) fn font_size(values: &ComputedValues) -> FontSize {
    FontSize::from_px(f64::from(values.clone_font_size().computed_size().px()))
        .unwrap_or(FontSize::INITIAL)
}

pub(super) fn cursor(values: &ComputedValues) -> Cursor {
    match values.clone_cursor().keyword {
        CursorKind::Auto => Cursor::Auto,
        CursorKind::Default => Cursor::Default,
        CursorKind::None => Cursor::None,
        CursorKind::ContextMenu => Cursor::ContextMenu,
        CursorKind::Help => Cursor::Help,
        CursorKind::Pointer => Cursor::Pointer,
        CursorKind::Progress => Cursor::Progress,
        CursorKind::Wait => Cursor::Wait,
        CursorKind::Cell => Cursor::Cell,
        CursorKind::Crosshair => Cursor::Crosshair,
        CursorKind::Text => Cursor::Text,
        CursorKind::VerticalText => Cursor::VerticalText,
        CursorKind::Alias => Cursor::Alias,
        CursorKind::Copy => Cursor::Copy,
        CursorKind::Move => Cursor::Move,
        CursorKind::NoDrop => Cursor::NoDrop,
        CursorKind::NotAllowed => Cursor::NotAllowed,
        CursorKind::Grab => Cursor::Grab,
        CursorKind::Grabbing => Cursor::Grabbing,
        CursorKind::EResize => Cursor::EResize,
        CursorKind::NResize => Cursor::NResize,
        CursorKind::NeResize => Cursor::NeResize,
        CursorKind::NwResize => Cursor::NwResize,
        CursorKind::SResize => Cursor::SResize,
        CursorKind::SeResize => Cursor::SeResize,
        CursorKind::SwResize => Cursor::SwResize,
        CursorKind::WResize => Cursor::WResize,
        CursorKind::EwResize => Cursor::EwResize,
        CursorKind::NsResize => Cursor::NsResize,
        CursorKind::NeswResize => Cursor::NeswResize,
        CursorKind::NwseResize => Cursor::NwseResize,
        CursorKind::ColResize => Cursor::ColResize,
        CursorKind::RowResize => Cursor::RowResize,
        CursorKind::AllScroll => Cursor::AllScroll,
        CursorKind::ZoomIn => Cursor::ZoomIn,
        CursorKind::ZoomOut => Cursor::ZoomOut,
    }
}

pub(super) fn visibility(values: &ComputedValues) -> Visibility {
    use longhands::visibility::computed_value::T as Value;

    match values.clone_visibility() {
        Value::Visible => Visibility::Visible,
        Value::Hidden => Visibility::Hidden,
        Value::Collapse => Visibility::Collapse,
    }
}

/// Only the keyword counter styles have a `ListStyleType`. A `@counter-style` name or a `<string>`
/// falls back to the initial value, as the custom cascade did for an unrecognised keyword.
pub(super) fn list_style_type(values: &ComputedValues) -> ListStyleType {
    use style::counter_style::CounterStyle;

    match &values.clone_list_style_type().0 {
        CounterStyle::None => ListStyleType::None,
        CounterStyle::Name(name) => list_style_type_name(&name.0).unwrap_or_default(),
        _ => ListStyleType::default(),
    }
}

pub(super) fn list_style_type_name(value: &str) -> Option<ListStyleType> {
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

pub(super) fn list_style_position(values: &ComputedValues) -> ListStylePosition {
    use longhands::list_style_position::computed_value::T as Value;

    match values.clone_list_style_position() {
        Value::Inside => ListStylePosition::Inside,
        Value::Outside => ListStylePosition::Outside,
    }
}

pub(super) fn table_layout(values: &ComputedValues) -> TableLayoutMode {
    use longhands::table_layout::computed_value::T as Value;

    match values.clone_table_layout() {
        Value::Auto => TableLayoutMode::Auto,
        Value::Fixed => TableLayoutMode::Fixed,
    }
}

pub(super) fn border_collapse(values: &ComputedValues) -> BorderCollapse {
    use longhands::border_collapse::computed_value::T as Value;

    match values.clone_border_collapse() {
        Value::Separate => BorderCollapse::Separate,
        Value::Collapse => BorderCollapse::Collapse,
    }
}

pub(super) fn caption_side(values: &ComputedValues) -> CaptionSide {
    use longhands::caption_side::computed_value::T as Value;

    match values.clone_caption_side() {
        Value::Top => CaptionSide::Top,
        Value::Bottom => CaptionSide::Bottom,
    }
}

pub(super) fn border_spacing(values: &ComputedValues, lengths: &Lengths) -> BorderSpacing {
    let spacing = values.clone_border_spacing();
    BorderSpacing::new(
        lengths.cells(spacing.0.width.0.px(), LengthAxis::Horizontal),
        lengths.cells(spacing.0.height.0.px(), LengthAxis::Vertical),
    )
}
