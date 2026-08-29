use style::color::{AbsoluteColor, ColorSpace};

use crate::core::style::{Rgb, Rgba};

/// The colour `UA_CSS` gives `html`, which the mapper reads back as "the theme decides".
///
/// `ComputedStyle::color` is `Option<Rgba>` and `None` means the frontend palette chooses, but
/// Stylo always computes a concrete colour: without a sentinel every element would arrive carrying
/// Stylo's initial black and the terminal themes would never apply. Declaring it in the user-agent
/// sheet rather than mutating the `Device`'s default computed values reaches every element by
/// ordinary inheritance and needs no `Arc::get_mut`.
///
/// Its one false negative is an author declaring literally this colour, which is why the value is
/// an unlikely one rather than a round number.
pub(in crate::css::stylo) const SENTINEL: Rgb = Rgb::new(1, 2, 3);

/// The `color` declaration that puts [`SENTINEL`] into the cascade.
pub(in crate::css::stylo) const SENTINEL_CSS: &str = "rgb(1, 2, 3)";

/// Convert a resolved absolute colour to sRGB bytes.
///
/// CIE and other non-sRGB spaces convert here rather than being refused, which is a deliberate
/// divergence from the custom cascade: it ignored those declarations rather than guess, and Stylo
/// simply knows how to convert them.
pub(super) fn rgba(color: &AbsoluteColor) -> Rgba {
    let srgb = color.to_color_space(ColorSpace::Srgb);
    Rgba {
        rgb: Rgb::new(
            channel(srgb.components.0),
            channel(srgb.components.1),
            channel(srgb.components.2),
        ),
        alpha: channel(srgb.alpha),
    }
}

/// The foreground colour, with the sentinel turned back into "the theme decides".
pub(super) fn foreground(color: &AbsoluteColor) -> Option<Rgba> {
    let value = rgba(color);
    (value.rgb != SENTINEL).then_some(value)
}

/// A background colour. Stylo's initial `background-color` is `transparent`, which is exactly our
/// `None`, so this needs no sentinel of its own.
pub(super) fn background(color: &AbsoluteColor) -> Option<Rgb> {
    let value = rgba(color);
    (value.alpha != 0).then_some(value.rgb)
}

/// A component in 0..=1, saturating rather than wrapping. Missing components are NaN in Stylo's
/// representation and read as zero.
fn channel(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}
