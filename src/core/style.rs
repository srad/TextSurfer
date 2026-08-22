use std::collections::HashMap;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    None,
    #[default]
    Inline,
    Block,
    Table,
    InlineTable,
    TableHeaderGroup,
    TableRowGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableColumn,
    TableColumnGroup,
    TableCaption,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssWidth {
    #[default]
    Auto,
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
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn relative_luminance(self) -> f32 {
        fn channel(value: u8) -> f32 {
            let value = f32::from(value) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    pub fn contrast_ratio(self, other: Self) -> f32 {
        let (bright, dark) = {
            let a = self.relative_luminance();
            let b = other.relative_luminance();
            if a >= b { (a, b) } else { (b, a) }
        };
        (bright + 0.05) / (dark + 0.05)
    }

    pub fn blend(self, target: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let mix = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
        };
        Self {
            r: mix(self.r, target.r),
            g: mix(self.g, target.g),
            b: mix(self.b, target.b),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub text: Rgb,
    pub background: Rgb,
    pub link: Rgb,
}

impl Palette {
    pub const DEFAULT: Self = Self {
        text: Rgb::WHITE,
        background: Rgb::BLACK,
        link: Rgb::new(0, 0, 238),
    };
}

impl Default for Palette {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    pub strike: bool,
    pub reverse: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeSizes {
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
    pub left: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableLayoutMode {
    #[default]
    Auto,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderSpacing {
    pub horizontal: usize,
    pub vertical: usize,
}

impl BorderSpacing {
    pub const fn new(horizontal: usize, vertical: usize) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionSide {
    #[default]
    Top,
    Bottom,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyle {
    pub display: Display,
    pub white_space: WhiteSpace,
    pub width: CssWidth,
    pub box_sizing: BoxSizing,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: BorderEdges,
    pub table_layout: TableLayoutMode,
    pub border_collapse: BorderCollapse,
    pub border_spacing: BorderSpacing,
    pub caption_side: CaptionSide,
    pub color: Option<Rgb>,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    pub strike: bool,
    pub reverse: bool,
}

impl ComputedStyle {
    pub fn cell_style(&self) -> CellStyle {
        CellStyle {
            fg: self.color,
            bg: self.background,
            bold: self.bold,
            underline: self.underline,
            strike: self.strike,
            reverse: self.reverse,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleTree {
    styles: HashMap<NodeId, ComputedStyle>,
}

impl StyleTree {
    pub fn insert(&mut self, node: NodeId, style: ComputedStyle) {
        self.styles.insert(node, style);
    }

    pub fn get(&self, node: NodeId) -> ComputedStyle {
        self.styles.get(&node).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_ratio_is_symmetric_and_bounded() {
        let ratio = Rgb::WHITE.contrast_ratio(Rgb::BLACK);
        assert!((ratio - 21.0).abs() < 0.01, "white on black is 21:1");
        assert_eq!(ratio, Rgb::BLACK.contrast_ratio(Rgb::WHITE));
        assert!((Rgb::WHITE.contrast_ratio(Rgb::WHITE) - 1.0).abs() < 0.001);
    }

    #[test]
    fn blending_walks_from_source_to_target() {
        let navy = Rgb::new(0, 0, 128);
        assert_eq!(navy.blend(Rgb::WHITE, 0.0), navy);
        assert_eq!(navy.blend(Rgb::WHITE, 1.0), Rgb::WHITE);
        let half = navy.blend(Rgb::WHITE, 0.5);
        assert!(half.r > navy.r && half.r < 255);
    }

    #[test]
    fn cell_style_projects_the_visual_half_of_a_computed_style() {
        let style = ComputedStyle {
            color: Some(Rgb::WHITE),
            bold: true,
            underline: true,
            ..Default::default()
        };
        let cell = style.cell_style();
        assert_eq!(cell.fg, Some(Rgb::WHITE));
        assert_eq!(cell.bg, None);
        assert!(cell.bold && cell.underline && !cell.strike && !cell.reverse);
    }
}
