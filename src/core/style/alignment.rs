use super::CssPercentage;

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

/// The CSS Box Alignment properties, which both the flex and the grid formatting contexts read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlignmentStyle {
    pub justify_content: Alignment<ContentAlignment>,
    pub align_content: Alignment<ContentAlignment>,
    pub justify_items: Alignment<ItemAlignment>,
    pub align_items: Alignment<ItemAlignment>,
    pub justify_self: Option<Alignment<ItemAlignment>>,
    pub align_self: Option<Alignment<ItemAlignment>>,
    pub row_gap: CssGap,
    pub column_gap: CssGap,
}

impl AlignmentStyle {
    const DEFAULT: Self = Self {
        justify_content: Alignment {
            keyword: ContentAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        align_content: Alignment {
            keyword: ContentAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        justify_items: Alignment {
            keyword: ItemAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        align_items: Alignment {
            keyword: ItemAlignment::Normal,
            safety: AlignmentSafety::Unsafe,
        },
        justify_self: None,
        align_self: None,
        row_gap: CssGap::Normal,
        column_gap: CssGap::Normal,
    };
}

impl Default for AlignmentStyle {
    fn default() -> Self {
        Self::DEFAULT
    }
}
