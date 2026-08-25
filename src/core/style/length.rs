use crate::core::geom::Size;

const MAX_LAYOUT_CELLS: f64 = u16::MAX as f64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CssLengthUnit {
    Px,
    In,
    Cm,
    Mm,
    Q,
    Pt,
    Pc,
    Em,
    Rem,
    Ex,
    Ch,
    Vw,
    Vh,
    Vmin,
    Vmax,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CssLength {
    value_bits: u32,
    unit: CssLengthUnit,
}

impl CssLength {
    pub fn new(value: f32, unit: CssLengthUnit) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        let value = if value == 0.0 { 0.0 } else { value };
        Some(Self {
            value_bits: value.to_bits(),
            unit,
        })
    }

    pub const fn zero() -> Self {
        Self {
            value_bits: 0,
            unit: CssLengthUnit::Px,
        }
    }

    pub fn value(self) -> f32 {
        f32::from_bits(self.value_bits)
    }

    pub const fn unit(self) -> CssLengthUnit {
        self.unit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LengthAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellMetric {
    column_px: u16,
    row_px: u16,
    root_font_px: u16,
}

impl CellMetric {
    pub const DEFAULT: Self = Self {
        column_px: 8,
        row_px: 16,
        root_font_px: 16,
    };

    pub const fn new(column_px: u16, row_px: u16, root_font_px: u16) -> Option<Self> {
        if column_px == 0 || row_px == 0 || root_font_px == 0 {
            return None;
        }
        Some(Self {
            column_px,
            row_px,
            root_font_px,
        })
    }

    pub const fn column_px(self) -> u16 {
        self.column_px
    }

    pub const fn row_px(self) -> u16 {
        self.row_px
    }

    pub const fn root_font_px(self) -> u16 {
        self.root_font_px
    }

    pub fn viewport_css_pixels(self, axis: LengthAxis, viewport: Size) -> f64 {
        match axis {
            LengthAxis::Horizontal => f64::from(viewport.cols) * f64::from(self.column_px),
            LengthAxis::Vertical => f64::from(viewport.rows) * f64::from(self.row_px),
        }
    }

    pub fn css_pixels(self, length: CssLength, viewport: Size) -> f64 {
        self.css_pixels_with_fonts(
            length,
            viewport,
            f64::from(self.root_font_px),
            f64::from(self.root_font_px),
        )
    }

    pub fn css_pixels_with_fonts(
        self,
        length: CssLength,
        viewport: Size,
        font_px: f64,
        root_font_px: f64,
    ) -> f64 {
        let width = self.viewport_css_pixels(LengthAxis::Horizontal, viewport);
        let height = self.viewport_css_pixels(LengthAxis::Vertical, viewport);
        let scale = match length.unit() {
            CssLengthUnit::Px => 1.0,
            CssLengthUnit::In => 96.0,
            CssLengthUnit::Cm => 96.0 / 2.54,
            CssLengthUnit::Mm => 96.0 / 25.4,
            CssLengthUnit::Q => 96.0 / 101.6,
            CssLengthUnit::Pt => 96.0 / 72.0,
            CssLengthUnit::Pc => 16.0,
            CssLengthUnit::Em => font_px,
            CssLengthUnit::Rem => root_font_px,
            CssLengthUnit::Ex | CssLengthUnit::Ch => font_px / 2.0,
            CssLengthUnit::Vw => width / 100.0,
            CssLengthUnit::Vh => height / 100.0,
            CssLengthUnit::Vmin => width.min(height) / 100.0,
            CssLengthUnit::Vmax => width.max(height) / 100.0,
        };
        f64::from(length.value()) * scale
    }

    pub fn resolve_cells(self, length: CssLength, axis: LengthAxis, viewport: Size) -> usize {
        self.resolve_cells_with_fonts(
            length,
            axis,
            viewport,
            f64::from(self.root_font_px),
            f64::from(self.root_font_px),
        )
    }

    pub fn resolve_cells_with_fonts(
        self,
        length: CssLength,
        axis: LengthAxis,
        viewport: Size,
        font_px: f64,
        root_font_px: f64,
    ) -> usize {
        let cell_px = match axis {
            LengthAxis::Horizontal => self.column_px,
            LengthAxis::Vertical => self.row_px,
        };
        (self.css_pixels_with_fonts(length, viewport, font_px, root_font_px) / f64::from(cell_px))
            .max(0.0)
            .round()
            .min(MAX_LAYOUT_CELLS) as usize
    }

    pub fn resolve_signed_cells_with_fonts(
        self,
        length: CssLength,
        axis: LengthAxis,
        viewport: Size,
        font_px: f64,
        root_font_px: f64,
    ) -> isize {
        let cell_px = match axis {
            LengthAxis::Horizontal => self.column_px,
            LengthAxis::Vertical => self.row_px,
        };
        (self.css_pixels_with_fonts(length, viewport, font_px, root_font_px) / f64::from(cell_px))
            .round()
            .clamp(-MAX_LAYOUT_CELLS, MAX_LAYOUT_CELLS) as isize
    }
}

impl Default for CellMetric {
    fn default() -> Self {
        Self::DEFAULT
    }
}
