use super::{CssCalc, CssNumber, CssPercentage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub const fn is_column(self) -> bool {
        matches!(self, Self::Column | Self::ColumnReverse)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AxisCellLength {
    pub horizontal: usize,
    pub vertical: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisCalc {
    pub horizontal: CssCalc,
    pub vertical: CssCalc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexBasis {
    #[default]
    Auto,
    Content,
    Cells(AxisCellLength),
    Percent(CssPercentage),
    Calc(AxisCalc),
}

impl FlexBasis {
    pub const fn zero() -> Self {
        Self::Cells(AxisCellLength {
            horizontal: 0,
            vertical: 0,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexStyle {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub grow: CssNumber,
    pub shrink: CssNumber,
    pub basis: FlexBasis,
}

impl FlexStyle {
    pub const fn none() -> Self {
        Self {
            grow: CssNumber::ZERO,
            shrink: CssNumber::ZERO,
            basis: FlexBasis::Auto,
            ..Self::DEFAULT
        }
    }

    const DEFAULT: Self = Self {
        direction: FlexDirection::Row,
        wrap: FlexWrap::NoWrap,
        grow: CssNumber::ZERO,
        shrink: CssNumber::ONE,
        basis: FlexBasis::Auto,
    };
}

impl Default for FlexStyle {
    fn default() -> Self {
        Self::DEFAULT
    }
}
