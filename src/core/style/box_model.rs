use super::Rgb;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssSize {
    #[default]
    Auto,
    Cells(usize),
    Percent(CssPercentage),
}

pub type CssWidth = CssSize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssMaxSize {
    #[default]
    None,
    Cells(usize),
    Percent(CssPercentage),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CssPercentage(u32);

impl CssPercentage {
    pub const fn new(basis_points: u32) -> Self {
        Self(basis_points)
    }

    pub const fn basis_points(self) -> u32 {
        self.0
    }

    pub fn resolve(self, basis: usize) -> usize {
        basis.saturating_mul(self.0 as usize).div_ceil(10_000)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeSizes {
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
    pub left: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssMargin {
    Auto,
    Cells(usize),
}

impl Default for CssMargin {
    fn default() -> Self {
        Self::ZERO
    }
}

impl CssMargin {
    pub const ZERO: Self = Self::Cells(0);

    pub const fn cells(self) -> usize {
        match self {
            Self::Auto => 0,
            Self::Cells(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarginEdges {
    pub top: CssMargin,
    pub right: CssMargin,
    pub bottom: CssMargin,
    pub left: CssMargin,
}

impl Default for MarginEdges {
    fn default() -> Self {
        Self {
            top: CssMargin::ZERO,
            right: CssMargin::ZERO,
            bottom: CssMargin::ZERO,
            left: CssMargin::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderLineStyle {
    #[default]
    None,
    Hidden,
    Inset,
    Groove,
    Outset,
    Ridge,
    Dotted,
    Dashed,
    Solid,
    Double,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderColor {
    #[default]
    CurrentColor,
    Transparent,
    Rgb(Rgb),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderSide {
    pub width: usize,
    pub style: BorderLineStyle,
    pub color: BorderColor,
}

impl BorderSide {
    pub const fn is_visible(self) -> bool {
        self.width > 0
            && !matches!(self.style, BorderLineStyle::None | BorderLineStyle::Hidden)
            && !matches!(self.color, BorderColor::Transparent)
    }

    pub const fn visible_width(self) -> usize {
        if self.is_visible() { 1 } else { 0 }
    }

    pub const fn layout_width(self) -> usize {
        if self.width > 0 && !matches!(self.style, BorderLineStyle::None | BorderLineStyle::Hidden)
        {
            1
        } else {
            0
        }
    }
}

impl Default for BorderSide {
    fn default() -> Self {
        Self {
            width: 1,
            style: BorderLineStyle::None,
            color: BorderColor::CurrentColor,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderEdges {
    pub top: BorderSide,
    pub right: BorderSide,
    pub bottom: BorderSide,
    pub left: BorderSide,
}

impl BorderEdges {
    pub const fn uniform(side: BorderSide) -> Self {
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    pub const fn is_visible(self) -> bool {
        self.top.is_visible()
            || self.right.is_visible()
            || self.bottom.is_visible()
            || self.left.is_visible()
    }

    pub const fn has_layout(self) -> bool {
        self.top.layout_width() > 0
            || self.right.layout_width() > 0
            || self.bottom.layout_width() > 0
            || self.left.layout_width() > 0
    }
}
