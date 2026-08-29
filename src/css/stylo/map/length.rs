use std::collections::HashMap;

use style::values::computed::LengthPercentage;
use style::values::computed::length_percentage::Unpacked;
use style_traits::ToCss;

use crate::core::geom::Size;
use crate::core::style::{CalcRange, CellMetric, CssCalc, LengthAxis, StyleStore};

/// The two axes a `<length-percentage>` is resolved against.
///
/// They are not always the same: a margin or padding percentage resolves against the containing
/// block's *inline* size whatever edge it sits on, while a size or an inset percentage resolves
/// against its own axis. Two bare `LengthAxis` arguments would swap silently at a call site, so the
/// pair travels as one named value.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Axes {
    pub(super) output: LengthAxis,
    pub(super) basis: LengthAxis,
}

impl Axes {
    /// Percentages resolve against the same axis they are emitted on.
    pub(super) const fn same(axis: LengthAxis) -> Self {
        Self {
            output: axis,
            basis: axis,
        }
    }

    /// Percentages resolve against the inline axis — the margin and padding rule.
    pub(super) const fn inline_basis(axis: LengthAxis) -> Self {
        Self {
            output: axis,
            basis: LengthAxis::Horizontal,
        }
    }
}

/// A computed `<length-percentage>` reduced to the three shapes `core::style` can hold.
///
/// Every target type — `CssSize`, `CssMargin`, `CssPadding`, `CssInset`, `CssGap`, `GridLength` —
/// is some subset of these, so the conversion happens once and each caller picks its variants.
pub(super) enum Value {
    /// Cells on the output axis. Signed; callers that forbid negatives reject or clamp.
    Cells(isize),
    /// A percentage as a 0..1 fraction, only ever produced when both axes agree.
    Percent(f32),
    Calc(CssCalc),
}

/// Converts Stylo's computed pixels into cells, and its computed `calc()` into our calc store.
pub(super) struct Lengths {
    cell: CellMetric,
    viewport: Size,
    /// Serialised `calc()` text is the cache key because the lowering is what is expensive, and a
    /// page applies the same handful of expressions to thousands of elements.
    calc: HashMap<CalcKey, Option<CssCalc>>,
}

#[derive(PartialEq, Eq, Hash)]
struct CalcKey {
    text: String,
    axes: Axes,
    range: CalcRange,
}

impl Lengths {
    pub(super) fn new(cell: CellMetric, viewport: Size) -> Self {
        Self {
            cell,
            viewport,
            calc: HashMap::new(),
        }
    }

    pub(super) fn cells(&self, px: f32, axis: LengthAxis) -> usize {
        self.cell.cells_from_px(f64::from(px), axis)
    }

    pub(super) fn signed_cells(&self, px: f32, axis: LengthAxis) -> isize {
        self.cell.signed_cells_from_px(f64::from(px), axis)
    }

    /// Reduce one computed `<length-percentage>`.
    ///
    /// Returns `None` only when a `calc()` cannot be represented — an unsupported math function or
    /// an exhausted store budget — which leaves the caller to fall back to the property's initial
    /// value. There is no declaration left to invalidate by this point.
    pub(super) fn resolve(
        &mut self,
        value: &LengthPercentage,
        axes: Axes,
        range: CalcRange,
        store: &mut StyleStore,
    ) -> Option<Value> {
        match value.unpack() {
            Unpacked::Length(length) => {
                Some(Value::Cells(self.signed_cells(length.px(), axes.output)))
            }
            // A percentage whose basis is a different axis cannot be a bare `Percent`: the stored
            // value is in output-axis cells, so the axis ratio has to be baked in, which only the
            // calc store can carry.
            Unpacked::Percentage(percentage) if axes.output == axes.basis => {
                Some(Value::Percent(percentage.0))
            }
            Unpacked::Percentage(_) | Unpacked::Calc(_) => {
                self.lower(value, axes, range, store).map(Value::Calc)
            }
        }
    }

    /// Lower a computed `calc()` by serialising it and re-parsing with `css::math`.
    ///
    /// `CalcLengthPercentage` keeps its node tree private and exposes only `resolve(basis)`, so
    /// there is nothing to walk. Probing at two bases would recover a linear expression and lie
    /// about `min()`, `max()` and `clamp()`. Serialisation is exact: `CalcNode::to_css` writes at
    /// calculation-root level, so a sum comes back wrapped in `calc(…)` and the others as their own
    /// functions, while a lone leaf comes back bare — all three forms `parse_length_percentage`
    /// accepts. The clamping mode is `#[css(skip)]` and does not survive, which is why `range`
    /// comes from the property instead.
    fn lower(
        &mut self,
        value: &LengthPercentage,
        axes: Axes,
        range: CalcRange,
        store: &mut StyleStore,
    ) -> Option<CssCalc> {
        let key = CalcKey {
            text: value.to_css_string(),
            axes,
            range,
        };
        if let Some(cached) = self.calc.get(&key) {
            return *cached;
        }
        let lowered =
            crate::css::math::parse_length_percentage_source(&key.text).and_then(|parsed| {
                let expression = parsed.lower_cells_with_metric(
                    self.cell,
                    self.viewport,
                    axes.output,
                    axes.basis,
                )?;
                store.calculations.insert(expression, range)
            });
        self.calc.insert(key, lowered);
        lowered
    }
}
