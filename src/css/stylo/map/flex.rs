use style::properties::ComputedValues;
use style::properties::longhands;
use style::values::generics::flex::GenericFlexBasis;

use crate::core::style::{
    AxisCalc, AxisCellLength, CalcRange, CssNumber, FlexBasis, FlexDirection, FlexStyle, FlexWrap,
    LengthAxis, StyleStore,
};

use super::length::{Axes, Lengths, Value};

pub(super) fn flex(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> FlexStyle {
    FlexStyle {
        direction: direction(values),
        wrap: wrap(values),
        grow: number(values.clone_flex_grow().0),
        shrink: number(values.clone_flex_shrink().0),
        basis: basis(values, lengths, store),
    }
}

fn direction(values: &ComputedValues) -> FlexDirection {
    use longhands::flex_direction::computed_value::T as Value;

    match values.clone_flex_direction() {
        Value::Row => FlexDirection::Row,
        Value::RowReverse => FlexDirection::RowReverse,
        Value::Column => FlexDirection::Column,
        Value::ColumnReverse => FlexDirection::ColumnReverse,
    }
}

fn wrap(values: &ComputedValues) -> FlexWrap {
    use longhands::flex_wrap::computed_value::T as Value;

    match values.clone_flex_wrap() {
        Value::Nowrap => FlexWrap::NoWrap,
        Value::Wrap => FlexWrap::Wrap,
        Value::WrapReverse => FlexWrap::WrapReverse,
    }
}

fn number(value: f32) -> CssNumber {
    CssNumber::new(value).unwrap_or(CssNumber::ZERO)
}

/// `flex-basis` is stored per axis, because the flex container's main axis is not known until
/// layout: a length basis is quantised against both cell metrics up front, and a `calc()` basis
/// takes two store entries for the same reason.
fn basis(values: &ComputedValues, lengths: &mut Lengths, store: &mut StyleStore) -> FlexBasis {
    let size = match values.clone_flex_basis() {
        GenericFlexBasis::Content => return FlexBasis::Content,
        GenericFlexBasis::Size(size) => size,
    };
    let style::values::computed::Size::LengthPercentage(length) = size else {
        return FlexBasis::Auto;
    };
    let horizontal = lengths.resolve(
        &length.0,
        Axes::same(LengthAxis::Horizontal),
        CalcRange::NonNegative,
        store,
    );
    match horizontal {
        Some(Value::Cells(cells)) => {
            let vertical = lengths.resolve(
                &length.0,
                Axes::same(LengthAxis::Vertical),
                CalcRange::NonNegative,
                store,
            );
            let Some(Value::Cells(rows)) = vertical else {
                return FlexBasis::Auto;
            };
            FlexBasis::Cells(AxisCellLength {
                horizontal: cells.max(0) as usize,
                vertical: rows.max(0) as usize,
            })
        }
        // A percentage is axis-free: layout resolves it against whichever axis turns out to be main.
        Some(Value::Percent(fraction)) => {
            FlexBasis::Percent(super::sizing::percentage(fraction.max(0.0)))
        }
        Some(Value::Calc(horizontal)) => {
            let vertical = lengths.resolve(
                &length.0,
                Axes::same(LengthAxis::Vertical),
                CalcRange::NonNegative,
                store,
            );
            let Some(Value::Calc(vertical)) = vertical else {
                return FlexBasis::Auto;
            };
            FlexBasis::Calc(AxisCalc {
                horizontal,
                vertical,
            })
        }
        None => FlexBasis::Auto,
    }
}
