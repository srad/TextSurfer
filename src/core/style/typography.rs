#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextRendering {
    #[default]
    Cell,
    ScaledBitmap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSize(u32);

impl FontSize {
    pub const INITIAL: Self = Self(16.0f32.to_bits());

    pub fn from_px(px: f64) -> Option<Self> {
        if !px.is_finite() || px < 0.0 {
            return None;
        }
        Some(Self((px.min(f64::from(u16::MAX)) as f32).to_bits()))
    }

    pub fn px(self) -> f64 {
        f64::from(f32::from_bits(self.0))
    }

    pub fn presentation(self, rendering: TextRendering) -> TextPresentation {
        if rendering == TextRendering::Cell {
            return TextPresentation::normal();
        }
        let px = self.px();
        if px == 0.0 {
            TextPresentation {
                scale: 0,
                dim: false,
            }
        } else if px >= 32.0 {
            TextPresentation {
                scale: 4,
                dim: false,
            }
        } else if px >= 24.0 {
            TextPresentation {
                scale: 3,
                dim: false,
            }
        } else if px >= 18.719 {
            TextPresentation {
                scale: 2,
                dim: false,
            }
        } else {
            TextPresentation {
                scale: 1,
                dim: px < 16.0,
            }
        }
    }
}

impl Default for FontSize {
    fn default() -> Self {
        Self::INITIAL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPresentation {
    pub scale: u8,
    pub dim: bool,
}

impl TextPresentation {
    pub const fn normal() -> Self {
        Self {
            scale: 1,
            dim: false,
        }
    }
}

impl Default for TextPresentation {
    fn default() -> Self {
        Self::normal()
    }
}
