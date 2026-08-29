use style::properties::ComputedValues;
use style::values::computed::length::NonNegativeLengthPercentageOrNormal;
use style::values::generics::length::GenericLengthPercentageOrNormal;
use style::values::specified::align::AlignFlags;

use crate::core::style::{
    Alignment, AlignmentSafety, AlignmentStyle, CalcRange, ContentAlignment, CssGap, ItemAlignment,
    LengthAxis, StyleStore,
};

use super::length::{Axes, Lengths, Value};

pub(super) fn alignment(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> AlignmentStyle {
    AlignmentStyle {
        justify_content: content(values.clone_justify_content().primary()),
        align_content: content(values.clone_align_content().primary()),
        justify_items: item(values.clone_justify_items().computed.0.0),
        align_items: item(values.clone_align_items().0),
        justify_self: self_alignment(values.clone_justify_self().0),
        align_self: self_alignment(values.clone_align_self().0),
        row_gap: gap(
            &values.clone_row_gap(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        column_gap: gap(
            &values.clone_column_gap(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
    }
}

/// `AlignFlags` is a keyword in its low five bits and `LEGACY`/`SAFE`/`UNSAFE` above them.
///
/// `LEGACY` only changes how table cells inherit `justify-items`, which we do not model, so it is
/// masked away with the rest and the keyword stands.
fn safety(flags: AlignFlags) -> AlignmentSafety {
    if flags.contains(AlignFlags::SAFE) {
        AlignmentSafety::Safe
    } else {
        AlignmentSafety::Unsafe
    }
}

/// `align-self` and `justify-self` are `auto` when the author said nothing, which is our `None`.
fn self_alignment(flags: AlignFlags) -> Option<Alignment<ItemAlignment>> {
    (flags.value() != AlignFlags::AUTO).then(|| item(flags))
}

/// Three `AlignFlags` values have no `ItemAlignment` counterpart.
///
/// `LEFT`/`RIGHT` fold to `Start`/`End`, matching `css/cascade/alignment.rs`, which accepts them
/// only on the inline axis and stores the logical keyword. `LAST_BASELINE` folds to `Baseline`
/// because we do not model last-baseline alignment. `ANCHOR_CENTER` cannot occur — anchor
/// positioning is pref-gated off — but folds to `Center` rather than reaching a panic.
fn item(flags: AlignFlags) -> Alignment<ItemAlignment> {
    let keyword = match flags.value() {
        AlignFlags::NORMAL | AlignFlags::AUTO => ItemAlignment::Normal,
        AlignFlags::STRETCH => ItemAlignment::Stretch,
        AlignFlags::START | AlignFlags::LEFT => ItemAlignment::Start,
        AlignFlags::END | AlignFlags::RIGHT => ItemAlignment::End,
        AlignFlags::FLEX_START => ItemAlignment::FlexStart,
        AlignFlags::FLEX_END => ItemAlignment::FlexEnd,
        AlignFlags::SELF_START => ItemAlignment::SelfStart,
        AlignFlags::SELF_END => ItemAlignment::SelfEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => ItemAlignment::Center,
        AlignFlags::BASELINE | AlignFlags::LAST_BASELINE => ItemAlignment::Baseline,
        _ => ItemAlignment::Normal,
    };
    Alignment {
        keyword,
        safety: safety(flags),
    }
}

/// `ContentAlignment` keeps `Left`/`Right` — unlike `ItemAlignment`, which folds them — but has no
/// baseline variant at all, and `align-content: baseline` is valid on the block axis. It folds to
/// `Normal`, the initial value, which is also all Taffy's `AlignContent` can express.
fn content(flags: AlignFlags) -> Alignment<ContentAlignment> {
    let keyword = match flags.value() {
        AlignFlags::NORMAL | AlignFlags::AUTO => ContentAlignment::Normal,
        AlignFlags::STRETCH => ContentAlignment::Stretch,
        AlignFlags::START => ContentAlignment::Start,
        AlignFlags::END => ContentAlignment::End,
        AlignFlags::FLEX_START => ContentAlignment::FlexStart,
        AlignFlags::FLEX_END => ContentAlignment::FlexEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => ContentAlignment::Center,
        AlignFlags::SPACE_BETWEEN => ContentAlignment::SpaceBetween,
        AlignFlags::SPACE_AROUND => ContentAlignment::SpaceAround,
        AlignFlags::SPACE_EVENLY => ContentAlignment::SpaceEvenly,
        AlignFlags::LEFT => ContentAlignment::Left,
        AlignFlags::RIGHT => ContentAlignment::Right,
        AlignFlags::BASELINE | AlignFlags::LAST_BASELINE => ContentAlignment::Normal,
        _ => ContentAlignment::Normal,
    };
    Alignment {
        keyword,
        safety: safety(flags),
    }
}

fn gap(
    value: &NonNegativeLengthPercentageOrNormal,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssGap {
    let GenericLengthPercentageOrNormal::LengthPercentage(length) = value else {
        return CssGap::Normal;
    };
    match lengths.resolve(&length.0, Axes::same(axis), CalcRange::NonNegative, store) {
        Some(Value::Cells(cells)) => CssGap::Cells(cells.max(0) as usize),
        Some(Value::Percent(fraction)) => {
            CssGap::Percent(super::sizing::percentage(fraction.max(0.0)))
        }
        Some(Value::Calc(handle)) => CssGap::Calc(handle),
        None => CssGap::Normal,
    }
}
