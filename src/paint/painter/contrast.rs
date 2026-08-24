use crate::core::style::{CellStyle, Palette, Rgb, Rgba};

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

pub fn resolve_cell_style(mut style: CellStyle, palette: Palette) -> CellStyle {
    if let Some(foreground) = style.fg {
        let background = style.bg.unwrap_or(palette.background);
        if foreground.alpha == 0 {
            style.fg = None;
            style.bold = false;
            style.underline = false;
            style.strike = false;
            style.reverse = false;
        } else {
            style.fg = Some(Rgba::opaque(legible_foreground(
                foreground.composite_over(background),
                background,
                palette,
            )));
        }
    }
    style
}
