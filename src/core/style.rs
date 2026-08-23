use std::collections::HashMap;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    None,
    #[default]
    Inline,
    Block,
    ListItem,
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

impl Display {
    /// Block containers hold a block formatting context of their own; list items are block
    /// containers that additionally carry a marker.
    pub const fn is_block_container(self) -> bool {
        matches!(self, Self::Block | Self::ListItem)
    }
}

/// The pseudo-elements the cascade can produce boxes for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PseudoElement {
    Before,
    Marker,
    After,
}

/// The marker glyph or numbering style of a list item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStyleType {
    None,
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

impl ListStyleType {
    pub const fn is_numeric(self) -> bool {
        !matches!(self, Self::None | Self::Disc | Self::Circle | Self::Square)
    }

    /// Renders `value` in this counter style, without the trailing separator.
    pub fn render(self, value: i64) -> String {
        match self {
            Self::None => String::new(),
            Self::Disc => "\u{2022}".to_string(),
            Self::Circle => "\u{25e6}".to_string(),
            Self::Square => "\u{25aa}".to_string(),
            Self::Decimal => value.to_string(),
            Self::DecimalLeadingZero => {
                if (0..10).contains(&value) {
                    format!("0{value}")
                } else {
                    value.to_string()
                }
            }
            Self::LowerAlpha => alphabetic(value, false),
            Self::UpperAlpha => alphabetic(value, true),
            Self::LowerRoman => roman(value, false),
            Self::UpperRoman => roman(value, true),
        }
    }
}

fn alphabetic(value: i64, upper: bool) -> String {
    if value < 1 {
        return value.to_string();
    }
    let base = if upper { b'A' } else { b'a' };
    let mut remaining = value;
    let mut letters = Vec::new();
    while remaining > 0 {
        let index = (remaining - 1) % 26;
        letters.push((base + index as u8) as char);
        remaining = (remaining - 1) / 26;
    }
    letters.iter().rev().collect()
}

fn roman(value: i64, upper: bool) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if !(1..4000).contains(&value) {
        return value.to_string();
    }
    let mut remaining = value;
    let mut out = String::new();
    for (amount, numeral) in NUMERALS {
        while remaining >= amount {
            out.push_str(numeral);
            remaining -= amount;
        }
    }
    if upper { out.to_uppercase() } else { out }
}

/// Where a list marker sits relative to the item's content box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub alpha: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, alpha: u8) -> Self {
        Self {
            rgb: Rgb::new(r, g, b),
            alpha,
        }
    }

    pub const fn opaque(rgb: Rgb) -> Self {
        Self { rgb, alpha: 255 }
    }

    pub fn composite_over(self, background: Rgb) -> Rgb {
        background.blend(self.rgb, f32::from(self.alpha) / 255.0)
    }
}

impl From<Rgb> for Rgba {
    fn from(value: Rgb) -> Self {
        Self::opaque(value)
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
    pub fg: Option<Rgba>,
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
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub color: Option<Rgba>,
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

/// Generated content for one pseudo-element of one originating element. The text is resolved by
/// the cascade (counters and `attr()` included), so layout only has to place it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PseudoBox {
    pub text: String,
    pub style: ComputedStyle,
}

/// A resolved list marker. `reserve` is the width of the marker field shared by every list item
/// with the same parent, so numbers line up on one content column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub text: String,
    pub reserve: usize,
    pub position: ListStylePosition,
    pub style: ComputedStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleTree {
    styles: HashMap<NodeId, ComputedStyle>,
    pseudo: HashMap<(NodeId, PseudoElement), PseudoBox>,
    markers: HashMap<NodeId, Marker>,
}

impl StyleTree {
    pub fn insert(&mut self, node: NodeId, style: ComputedStyle) {
        self.styles.insert(node, style);
    }

    pub fn get(&self, node: NodeId) -> ComputedStyle {
        self.styles.get(&node).copied().unwrap_or_default()
    }

    pub fn insert_pseudo(&mut self, node: NodeId, which: PseudoElement, box_: PseudoBox) {
        self.pseudo.insert((node, which), box_);
    }

    pub fn pseudo(&self, node: NodeId, which: PseudoElement) -> Option<&PseudoBox> {
        self.pseudo.get(&(node, which))
    }

    pub fn insert_marker(&mut self, node: NodeId, marker: Marker) {
        self.markers.insert(node, marker);
    }

    pub fn marker(&self, node: NodeId) -> Option<&Marker> {
        self.markers.get(&node)
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
            color: Some(Rgba::opaque(Rgb::WHITE)),
            bold: true,
            underline: true,
            ..Default::default()
        };
        let cell = style.cell_style();
        assert_eq!(cell.fg, Some(Rgba::opaque(Rgb::WHITE)));
        assert_eq!(cell.bg, None);
        assert!(cell.bold && cell.underline && !cell.strike && !cell.reverse);
    }
}
