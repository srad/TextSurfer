use style::properties::ComputedValues;
use style::properties::longhands;
use style::values::computed::{MaxSize, Size};
use style::values::generics::box_::PositionProperty;
use style::values::specified::box_::{Clear as StyloClear, Float as StyloFloat, Overflow};

use crate::core::style::{
    BoxSizing, CalcRange, Clear, CssFloat, CssMaxSize, CssPercentage, CssSize, LengthAxis,
    Overflow as CellOverflow, OverflowAxes, Position, StyleStore,
};

use super::length::{Axes, Lengths, Value};

/// `width`, `height`, `min-width` and `min-height`.
///
/// The intrinsic keywords — `min-content`, `max-content`, `fit-content`, `stretch` and
/// `fit-content()` — have no `CssSize` variant and become `Auto`. The custom cascade dropped those
/// declarations instead and kept whatever was cascaded before; that difference is in the M7
/// divergence ledger. Both `layout.css.fit-content-function.enabled` and
/// `layout.css.stretch-size-keyword.enabled` default true, so they are reachable.
pub(super) fn size(
    value: &Size,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssSize {
    let Size::LengthPercentage(length) = value else {
        return CssSize::Auto;
    };
    match lengths.resolve(&length.0, Axes::same(axis), CalcRange::NonNegative, store) {
        Some(Value::Cells(cells)) => CssSize::Cells(cells.max(0) as usize),
        Some(Value::Percent(fraction)) => CssSize::Percent(percentage(fraction)),
        Some(Value::Calc(handle)) => CssSize::Calc(handle),
        None => CssSize::Auto,
    }
}

pub(super) fn max_size(
    value: &MaxSize,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssMaxSize {
    let MaxSize::LengthPercentage(length) = value else {
        return CssMaxSize::None;
    };
    match lengths.resolve(&length.0, Axes::same(axis), CalcRange::NonNegative, store) {
        Some(Value::Cells(cells)) => CssMaxSize::Cells(cells.max(0) as usize),
        Some(Value::Percent(fraction)) => CssMaxSize::Percent(percentage(fraction)),
        Some(Value::Calc(handle)) => CssMaxSize::Calc(handle),
        None => CssMaxSize::None,
    }
}

/// A 0..1 fraction as basis points, the representation `CssPercentage` holds.
pub(super) fn percentage(fraction: f32) -> CssPercentage {
    CssPercentage::new(
        (f64::from(fraction) * 10_000.0)
            .round()
            .clamp(0.0, f64::from(u32::MAX)) as u32,
    )
}

pub(super) fn box_sizing(values: &ComputedValues) -> BoxSizing {
    use longhands::box_sizing::computed_value::T as Value;

    match values.clone_box_sizing() {
        Value::ContentBox => BoxSizing::ContentBox,
        Value::BorderBox => BoxSizing::BorderBox,
    }
}

pub(super) fn position(values: &ComputedValues) -> Position {
    match values.clone_position() {
        PositionProperty::Static => Position::Static,
        PositionProperty::Relative => Position::Relative,
        PositionProperty::Absolute => Position::Absolute,
        PositionProperty::Fixed => Position::Fixed,
        PositionProperty::Sticky => Position::Sticky,
    }
}

/// The two axes plus the cross-axis fixup: a specified `visible` computes to `auto` when the other
/// axis is scrollable.
pub(super) fn overflow(values: &ComputedValues) -> OverflowAxes {
    OverflowAxes {
        x: axis_overflow(values.clone_overflow_x()),
        y: axis_overflow(values.clone_overflow_y()),
    }
    .computed()
}

fn axis_overflow(value: Overflow) -> CellOverflow {
    match value {
        Overflow::Visible => CellOverflow::Visible,
        Overflow::Hidden => CellOverflow::Hidden,
        Overflow::Scroll => CellOverflow::Scroll,
        Overflow::Auto => CellOverflow::Auto,
        Overflow::Clip => CellOverflow::Clip,
    }
}

/// The logical values fold to physical ones: `layout.writing-mode.enabled` is off and LTR-only is a
/// locked non-goal, so inline-start is always left.
pub(super) fn float(values: &ComputedValues) -> CssFloat {
    match values.clone_float() {
        StyloFloat::None => CssFloat::None,
        StyloFloat::Left | StyloFloat::InlineStart => CssFloat::Left,
        StyloFloat::Right | StyloFloat::InlineEnd => CssFloat::Right,
    }
}

pub(super) fn clear(values: &ComputedValues) -> Clear {
    match values.clone_clear() {
        StyloClear::None => Clear::None,
        StyloClear::Left | StyloClear::InlineStart => Clear::Left,
        StyloClear::Right | StyloClear::InlineEnd => Clear::Right,
        StyloClear::Both => Clear::Both,
    }
}
