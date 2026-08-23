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
