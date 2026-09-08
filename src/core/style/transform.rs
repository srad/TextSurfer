use super::{CssCalc, CssSignedPercentage, StyleTree};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssTranslationAxis {
    #[default]
    Zero,
    Cells(isize),
    Percent(CssSignedPercentage),
    Calc(CssCalc),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CssTranslation {
    pub x: CssTranslationAxis,
    pub y: CssTranslationAxis,
}

impl StyleTree {
    pub fn resolve_translation(
        &self,
        translation: CssTranslation,
        width: f32,
        height: f32,
    ) -> (f32, f32) {
        (
            self.resolve_translation_axis(translation.x, width),
            self.resolve_translation_axis(translation.y, height),
        )
    }

    fn resolve_translation_axis(&self, value: CssTranslationAxis, basis: f32) -> f32 {
        match value {
            CssTranslationAxis::Zero => 0.0,
            CssTranslationAxis::Cells(value) => value as f32,
            CssTranslationAxis::Percent(value) => {
                (basis * value.basis_points() as f32 / 10_000.0).round()
            }
            CssTranslationAxis::Calc(value) => {
                self.resolve_calc(value, basis).unwrap_or(0.0).round()
            }
        }
    }
}
