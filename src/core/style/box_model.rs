use super::{CssCalc, Rgb};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssSize {
    #[default]
    Auto,
    Cells(usize),
    Percent(CssPercentage),
    Calc(CssCalc),
}

pub type CssWidth = CssSize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssMaxSize {
    #[default]
    None,
    Cells(usize),
    Percent(CssPercentage),
    Calc(CssCalc),
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
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl Overflow {
    pub const fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword.as_bytes() {
            b"visible" => Self::Visible,
            b"hidden" => Self::Hidden,
            b"clip" => Self::Clip,
            b"scroll" => Self::Scroll,
            b"auto" => Self::Auto,
            _ => return None,
        })
    }

    pub const fn clips(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub const fn is_scrollable(self) -> bool {
        matches!(self, Self::Hidden | Self::Scroll | Self::Auto)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverflowAxes {
    pub x: Overflow,
    pub y: Overflow,
}

impl OverflowAxes {
    pub const VISIBLE: Self = Self {
        x: Overflow::Visible,
        y: Overflow::Visible,
    };

    pub const fn uniform(value: Overflow) -> Self {
        Self { x: value, y: value }
    }

    pub const fn computed(self) -> Self {
        match (self.x.is_scrollable(), self.y.is_scrollable()) {
            (true, false) => Self {
                x: self.x,
                y: Overflow::Auto,
            },
            (false, true) => Self {
                x: Overflow::Auto,
                y: self.y,
            },
            _ => self,
        }
    }

    pub const fn clips(self) -> bool {
        self.x.clips() || self.y.clips()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub const fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword.as_bytes() {
            b"static" => Self::Static,
            b"relative" => Self::Relative,
            b"absolute" => Self::Absolute,
            b"fixed" => Self::Fixed,
            b"sticky" => Self::Sticky,
            _ => return None,
        })
    }

    pub const fn is_absolute(self) -> bool {
        matches!(self, Self::Absolute | Self::Fixed)
    }

    pub const fn is_positioned(self) -> bool {
        !matches!(self, Self::Static)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssInset {
    #[default]
    Auto,
    Cells(isize),
    Percent(CssPercentage),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InsetEdges {
    pub top: CssInset,
    pub right: CssInset,
    pub bottom: CssInset,
    pub left: CssInset,
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
    Cells(isize),
}

impl Default for CssMargin {
    fn default() -> Self {
        Self::ZERO
    }
}

impl CssMargin {
    pub const ZERO: Self = Self::Cells(0);

    pub const fn cells(self) -> isize {
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
