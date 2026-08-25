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

#[test]
fn css_flex_numbers_accept_only_finite_nonnegative_values_and_canonicalize_zero() {
    assert_eq!(CssNumber::new(0.0), Some(CssNumber::ZERO));
    assert_eq!(CssNumber::new(-0.0), Some(CssNumber::ZERO));
    assert_eq!(CssNumber::new(1.0), Some(CssNumber::ONE));
    assert_eq!(CssNumber::new(2.5).unwrap().get(), 2.5);
    assert_eq!(CssNumber::new(-1.0), None);
    assert_eq!(CssNumber::new(f32::INFINITY), None);
    assert_eq!(CssNumber::new(f32::NEG_INFINITY), None);
    assert_eq!(CssNumber::new(f32::NAN), None);
}

#[test]
fn flex_defaults_distinguish_initial_items_from_flex_none() {
    let initial = FlexStyle::default();
    assert_eq!(initial.grow, CssNumber::ZERO);
    assert_eq!(initial.shrink, CssNumber::ONE);
    assert_eq!(initial.basis, FlexBasis::Auto);
    assert_eq!(initial.row_gap.cells(), None);
    assert_eq!(initial.column_gap.cells(), None);
    let none = FlexStyle::none();
    assert_eq!(none.grow, CssNumber::ZERO);
    assert_eq!(none.shrink, CssNumber::ZERO);
    assert_eq!(none.basis, FlexBasis::Auto);
    assert_eq!(none.direction, initial.direction);
    assert_eq!(none.align_items, initial.align_items);
}

#[test]
fn css_lengths_resolve_against_the_nominal_cell_and_viewport() {
    let metric = CellMetric::DEFAULT;
    let viewport = crate::core::geom::Size { cols: 80, rows: 24 };
    let cells = |value, unit, axis| {
        metric.resolve_cells(CssLength::new(value, unit).unwrap(), axis, viewport)
    };
    assert_eq!(cells(8.0, CssLengthUnit::Px, LengthAxis::Horizontal), 1);
    assert_eq!(cells(16.0, CssLengthUnit::Px, LengthAxis::Vertical), 1);
    assert_eq!(cells(1.0, CssLengthUnit::In, LengthAxis::Horizontal), 12);
    assert_eq!(cells(2.54, CssLengthUnit::Cm, LengthAxis::Horizontal), 12);
    assert_eq!(cells(25.4, CssLengthUnit::Mm, LengthAxis::Horizontal), 12);
    assert_eq!(cells(101.6, CssLengthUnit::Q, LengthAxis::Horizontal), 12);
    assert_eq!(cells(72.0, CssLengthUnit::Pt, LengthAxis::Horizontal), 12);
    assert_eq!(cells(6.0, CssLengthUnit::Pc, LengthAxis::Horizontal), 12);
    assert_eq!(cells(1.0, CssLengthUnit::Em, LengthAxis::Horizontal), 2);
    assert_eq!(cells(1.0, CssLengthUnit::Rem, LengthAxis::Vertical), 1);
    assert_eq!(cells(2.0, CssLengthUnit::Ex, LengthAxis::Horizontal), 2);
    assert_eq!(cells(1.0, CssLengthUnit::Ch, LengthAxis::Horizontal), 1);
    assert_eq!(cells(50.0, CssLengthUnit::Vw, LengthAxis::Horizontal), 40);
    assert_eq!(cells(50.0, CssLengthUnit::Vh, LengthAxis::Vertical), 12);
    assert_eq!(
        cells(100.0, CssLengthUnit::Vmin, LengthAxis::Horizontal),
        48
    );
    assert_eq!(cells(100.0, CssLengthUnit::Vmax, LengthAxis::Vertical), 40);
}

#[test]
fn css_length_cell_rounding_is_half_up_and_capped_at_u16_max() {
    let metric = CellMetric::DEFAULT;
    let viewport = crate::core::geom::Size { cols: 80, rows: 24 };
    assert_eq!(
        metric.resolve_cells(
            CssLength::new(4.0, CssLengthUnit::Px).unwrap(),
            LengthAxis::Horizontal,
            viewport,
        ),
        1
    );
    assert_eq!(
        metric.resolve_cells(
            CssLength::new(1_000_000.0, CssLengthUnit::Px).unwrap(),
            LengthAxis::Horizontal,
            viewport,
        ),
        u16::MAX as usize
    );
}
