use style::properties::ComputedValues;
use style::values::computed::{Inset, Margin, NonNegativeLengthPercentage};
use style::values::specified::border::BorderStyle;

use crate::core::style::{
    BorderColor, BorderEdges, BorderLineStyle, BorderSide, CalcRange, CssInset, CssMargin,
    CssPadding, CssSignedPercentage, InsetEdges, LengthAxis, MarginEdges, PaddingEdges, StyleStore,
};

use super::color;
use super::length::{Axes, Lengths, Value};

/// Insets resolve percentages against **their own axis**, unlike margins and paddings.
///
/// `css/cascade/declaration.rs::resolve_inset` lowers them with `(axis, axis)`, and a percentage
/// stays a plain `CssInset::Percent` on both axes as a result.
pub(super) fn insets(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> InsetEdges {
    InsetEdges {
        top: inset(&values.clone_top(), LengthAxis::Vertical, lengths, store),
        right: inset(
            &values.clone_right(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
        bottom: inset(&values.clone_bottom(), LengthAxis::Vertical, lengths, store),
        left: inset(&values.clone_left(), LengthAxis::Horizontal, lengths, store),
    }
}

fn inset(
    value: &Inset,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssInset {
    // The anchor-positioning variants cannot occur: `layout.css.anchor-positioning.enabled` is off.
    let Inset::LengthPercentage(length) = value else {
        return CssInset::Auto;
    };
    match lengths.resolve(length, Axes::same(axis), CalcRange::Unbounded, store) {
        Some(Value::Cells(cells)) => CssInset::Cells(cells),
        Some(Value::Percent(fraction)) => CssInset::Percent(signed_percentage(fraction)),
        Some(Value::Calc(handle)) => CssInset::Calc(handle),
        None => CssInset::Auto,
    }
}

/// Margins resolve percentages against the **inline** axis whatever edge they sit on.
///
/// That is why a vertical percentage margin cannot be a bare `CssMargin::Percent`: the stored value
/// is in row cells while the basis is measured in columns, so the ratio has to be baked in.
/// `Lengths::resolve` refuses to hand back a `Percent` when the axes differ, so passing
/// [`Axes::inline_basis`] is the whole of the correctness here.
pub(super) fn margins(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> MarginEdges {
    MarginEdges {
        top: margin(
            &values.clone_margin_top(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        right: margin(
            &values.clone_margin_right(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
        bottom: margin(
            &values.clone_margin_bottom(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        left: margin(
            &values.clone_margin_left(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
    }
}

fn margin(
    value: &Margin,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssMargin {
    let Margin::LengthPercentage(length) = value else {
        return CssMargin::Auto;
    };
    match lengths.resolve(
        length,
        Axes::inline_basis(axis),
        CalcRange::Unbounded,
        store,
    ) {
        Some(Value::Cells(cells)) => CssMargin::Cells(cells),
        Some(Value::Percent(fraction)) => CssMargin::Percent(signed_percentage(fraction)),
        Some(Value::Calc(handle)) => CssMargin::Calc(handle),
        None => CssMargin::ZERO,
    }
}

/// Paddings share the margins' inline percentage basis, but cannot be negative or `auto`.
pub(super) fn paddings(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> PaddingEdges {
    PaddingEdges {
        top: padding(
            &values.clone_padding_top(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        right: padding(
            &values.clone_padding_right(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
        bottom: padding(
            &values.clone_padding_bottom(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        left: padding(
            &values.clone_padding_left(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
    }
}

fn padding(
    value: &NonNegativeLengthPercentage,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> CssPadding {
    match lengths.resolve(
        &value.0,
        Axes::inline_basis(axis),
        CalcRange::NonNegative,
        store,
    ) {
        // `CssPadding` spells zero two ways: `Zero` is the enum default an undeclared edge keeps,
        // and `Cells(0)` is what resolving `padding: 0` produces. They are the same value —
        // `CssPadding::cells()` returns `Some(0)` for both — so the mapper emits one spelling
        // uniformly rather than trying to guess which one the other engine would have used.
        Some(Value::Cells(cells)) => CssPadding::Cells(cells.max(0) as usize),
        Some(Value::Percent(fraction)) => {
            CssPadding::Percent(super::sizing::percentage(fraction.max(0.0)))
        }
        Some(Value::Calc(handle)) => CssPadding::Calc(handle),
        None => CssPadding::Zero,
    }
}

pub(super) fn borders(values: &ComputedValues) -> BorderEdges {
    BorderEdges {
        top: BorderSide {
            width: binary_width(values.clone_border_top_width().0.to_f64_px()),
            style: line_style(values.clone_border_top_style()),
            color: border_color(values, &values.clone_border_top_color()),
        },
        right: BorderSide {
            width: binary_width(values.clone_border_right_width().0.to_f64_px()),
            style: line_style(values.clone_border_right_style()),
            color: border_color(values, &values.clone_border_right_color()),
        },
        bottom: BorderSide {
            width: binary_width(values.clone_border_bottom_width().0.to_f64_px()),
            style: line_style(values.clone_border_bottom_style()),
            color: border_color(values, &values.clone_border_bottom_color()),
        },
        left: BorderSide {
            width: binary_width(values.clone_border_left_width().0.to_f64_px()),
            style: line_style(values.clone_border_left_style()),
            color: border_color(values, &values.clone_border_left_color()),
        },
    }
}

/// **Border widths are binary, never quantised.**
///
/// `BorderSide::width` is a boolean in disguise — `css/values.rs` computes it as
/// `usize::from(width > 0.0)` and maps `thin`/`medium`/`thick` alike to 1 — because the terminal
/// paints one box-drawing frame for any non-zero width. Running it through `cells_from_px` instead
/// would turn Stylo's 3px `medium`, which is what an undeclared width computes to, into `round(3/8)
/// = 0` and erase the frame from every box that declares a style without a width.
fn binary_width(px: f64) -> usize {
    usize::from(px > 0.0)
}

fn line_style(value: BorderStyle) -> BorderLineStyle {
    match value {
        BorderStyle::None => BorderLineStyle::None,
        BorderStyle::Hidden => BorderLineStyle::Hidden,
        BorderStyle::Inset => BorderLineStyle::Inset,
        BorderStyle::Groove => BorderLineStyle::Groove,
        BorderStyle::Outset => BorderLineStyle::Outset,
        BorderStyle::Ridge => BorderLineStyle::Ridge,
        BorderStyle::Dotted => BorderLineStyle::Dotted,
        BorderStyle::Dashed => BorderLineStyle::Dashed,
        BorderStyle::Solid => BorderLineStyle::Solid,
        BorderStyle::Double => BorderLineStyle::Double,
    }
}

/// A border colour resolved against `currentColor`.
///
/// The colour sentinel does double duty here: an element whose `color` is still the theme's leaves
/// its borders reading as `CurrentColor`, which is exactly what an unstyled border means.
fn border_color(values: &ComputedValues, value: &style::values::computed::Color) -> BorderColor {
    let resolved = values.resolve_color(value);
    let rgba = color::rgba(&resolved);
    if rgba.alpha == 0 {
        return BorderColor::Transparent;
    }
    if rgba.rgb == color::SENTINEL {
        return BorderColor::CurrentColor;
    }
    BorderColor::Rgb(rgba.rgb)
}

fn signed_percentage(fraction: f32) -> CssSignedPercentage {
    CssSignedPercentage::new(
        (f64::from(fraction) * 10_000.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
    )
}
