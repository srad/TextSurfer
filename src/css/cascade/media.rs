use crate::core::geom::Size;
use crate::core::style::{CellMetric, FontSize, Palette, RenderContext, TextRendering};
#[cfg(test)]
use crate::core::style::{CssLength, LengthAxis};
use crate::css::{ColorScheme, DynamicState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaContext {
    pub palette: Palette,
    pub state: DynamicState,
    pub scripting: bool,
    pub color_scheme: ColorScheme,
    pub viewport: Size,
    pub cell_metric: CellMetric,
    pub text_rendering: TextRendering,
    pub font_size: FontSize,
    pub root_font_size: FontSize,
}

impl MediaContext {
    pub const fn screen() -> Self {
        Self {
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
            scripting: false,
            color_scheme: ColorScheme::Dark,
            viewport: Size { cols: 80, rows: 24 },
            cell_metric: CellMetric::DEFAULT,
            text_rendering: TextRendering::Cell,
            font_size: FontSize::INITIAL,
            root_font_size: FontSize::INITIAL,
        }
    }

    pub fn with_palette(self, palette: Palette) -> Self {
        Self { palette, ..self }
    }

    pub fn with_state(self, state: DynamicState) -> Self {
        Self { state, ..self }
    }

    pub fn with_scripting(self, scripting: bool) -> Self {
        Self { scripting, ..self }
    }

    pub fn with_color_scheme(self, color_scheme: ColorScheme) -> Self {
        Self {
            color_scheme,
            ..self
        }
    }

    pub fn with_viewport(self, viewport: Size) -> Self {
        Self { viewport, ..self }
    }

    pub fn with_cell_metric(self, cell_metric: CellMetric) -> Self {
        Self {
            cell_metric,
            ..self
        }
    }

    pub fn with_text_rendering(self, text_rendering: TextRendering) -> Self {
        Self {
            text_rendering,
            ..self
        }
    }

    pub fn with_render_context(self, render: RenderContext) -> Self {
        Self {
            viewport: render.viewport,
            cell_metric: render.metrics.cell,
            text_rendering: render.metrics.text,
            ..self
        }
    }

    #[cfg(test)]
    pub(in crate::css) fn resolve_fractional_cells(
        self,
        length: CssLength,
        axis: LengthAxis,
    ) -> f32 {
        let cell_px = match axis {
            LengthAxis::Horizontal => self.cell_metric.column_px(),
            LengthAxis::Vertical => self.cell_metric.row_px(),
        };
        (self.cell_metric.css_pixels_with_fonts(
            length,
            self.viewport,
            f64::from(self.cell_metric.root_font_px()),
            f64::from(self.cell_metric.root_font_px()),
        ) / f64::from(cell_px)) as f32
    }
}
