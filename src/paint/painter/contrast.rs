use crate::core::style::{CellStyle, Palette, Rgb};

pub(super) const MIN_CONTRAST: f32 = 3.0;

pub fn legible_foreground(foreground: Rgb, background: Rgb, palette: Palette) -> Rgb {
    if foreground.contrast_ratio(background) >= MIN_CONTRAST {
        return foreground;
    }
    let target = if palette.text.contrast_ratio(background) >= MIN_CONTRAST {
        palette.text
    } else if background.relative_luminance() > 0.4 {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    };
    for step in 1..=10u8 {
        let candidate = foreground.blend(target, f32::from(step) / 10.0);
        if candidate.contrast_ratio(background) >= MIN_CONTRAST {
            return candidate;
        }
    }
    target
}

pub(super) fn merge_style(under: CellStyle, over: CellStyle) -> CellStyle {
    CellStyle {
        fg: over.fg,
        bg: over.bg.or(under.bg),
        ..over
    }
}
