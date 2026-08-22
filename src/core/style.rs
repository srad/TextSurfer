use std::collections::HashMap;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    None,
    #[default]
    Inline,
    Block,
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
pub struct ComputedStyle {
    pub display: Display,
    pub white_space: WhiteSpace,
    pub width: CssWidth,
    pub box_sizing: BoxSizing,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: bool,
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
