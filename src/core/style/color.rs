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
