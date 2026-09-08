use style::properties::ComputedValues;
use style::values::computed::LengthPercentage;
use style::values::computed::transform::TransformOperation;

use crate::core::style::{
    CalcRange, CssSignedPercentage, CssTranslation, CssTranslationAxis, LengthAxis, StyleStore,
};

use super::length::{Axes, Lengths, Value};

pub(super) fn translation(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssTranslation {
    let transform = values.clone_transform();
    if transform.0.len() != 1 {
        return CssTranslation::default();
    }
    match &transform.0[0] {
        TransformOperation::Translate(x, y) => match (
            axis(x, LengthAxis::Horizontal, lengths, store),
            axis(y, LengthAxis::Vertical, lengths, store),
        ) {
            (Some(x), Some(y)) => CssTranslation { x, y },
            _ => CssTranslation::default(),
        },
        TransformOperation::TranslateX(x) => axis(x, LengthAxis::Horizontal, lengths, store)
            .map(|x| CssTranslation {
                x,
                y: CssTranslationAxis::Zero,
            })
            .unwrap_or_default(),
        TransformOperation::TranslateY(y) => axis(y, LengthAxis::Vertical, lengths, store)
            .map(|y| CssTranslation {
                x: CssTranslationAxis::Zero,
                y,
            })
            .unwrap_or_default(),
        _ => CssTranslation::default(),
    }
}

fn axis(
    value: &LengthPercentage,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<CssTranslationAxis> {
    match lengths.resolve(value, Axes::same(axis), CalcRange::Unbounded, store) {
        Some(Value::Cells(0)) | Some(Value::Percent(0.0)) => Some(CssTranslationAxis::Zero),
        Some(Value::Cells(value)) => Some(CssTranslationAxis::Cells(value)),
        Some(Value::Percent(value)) => Some(CssTranslationAxis::Percent(CssSignedPercentage::new(
            (f64::from(value) * 10_000.0)
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ))),
        Some(Value::Calc(value)) => Some(CssTranslationAxis::Calc(value)),
        None => None,
    }
}
