use super::CssPercentage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssNumber(u32);

impl CssNumber {
    pub const ZERO: Self = Self(0.0f32.to_bits());
    pub const ONE: Self = Self(1.0f32.to_bits());

    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then(|| {
            if value == 0.0 {
                Self::ZERO
            } else {
                Self(value.to_bits())
            }
        })
    }

    pub const fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl Default for CssNumber {
    fn default() -> Self {
        Self::ZERO
    }
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexBasis {
    #[default]
    Auto,
    Content,
    Cells(AxisCellLength),
    Percent(CssPercentage),
}

impl FlexBasis {
    pub const fn zero() -> Self {
        Self::Cells(AxisCellLength {
            horizontal: 0,
            vertical: 0,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssGap {
    #[default]
    Normal,
    Cells(usize),
    Percent(CssPercentage),
}

impl CssGap {
    pub const fn cells(self) -> Option<usize> {
        match self {
            Self::Cells(value) => Some(value),
            Self::Normal | Self::Percent(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignmentSafety {
    #[default]
    Unsafe,
    Safe,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemAlignment {
    #[default]
    Normal,
    Stretch,
    Start,
    End,
    FlexStart,
    FlexEnd,
    SelfStart,
    SelfEnd,
    Center,
    Baseline,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentAlignment {
    #[default]
    Normal,
    Stretch,
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alignment<T> {
    pub keyword: T,
    pub safety: AlignmentSafety,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexStyle {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub grow: CssNumber,
    pub shrink: CssNumber,
    pub basis: FlexBasis,
    pub order: i32,
    pub justify_content: Alignment<ContentAlignment>,
    pub align_items: Alignment<ItemAlignment>,
    pub align_self: Option<Alignment<ItemAlignment>>,
    pub align_content: Alignment<ContentAlignment>,
    pub row_gap: CssGap,
    pub column_gap: CssGap,
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
        order: 0,
        justify_content: Alignment {
            keyword: ContentAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        align_items: Alignment {
            keyword: ItemAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        align_self: None,
        align_content: Alignment {
            keyword: ContentAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        row_gap: CssGap::Normal,
        column_gap: CssGap::Normal,
    };
}

impl Default for FlexStyle {
    fn default() -> Self {
        Self::DEFAULT
    }
}
