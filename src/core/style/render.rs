use crate::core::geom::Size;

use super::{CellMetric, TextRendering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderMetrics {
    pub cell: CellMetric,
    pub text: TextRendering,
}

impl RenderMetrics {
    pub const TERMINAL: Self = Self {
        cell: CellMetric::DEFAULT,
        text: TextRendering::Cell,
    };

    pub const VGA: Self = Self {
        cell: CellMetric::DEFAULT,
        text: TextRendering::ScaledBitmap,
    };
}

impl Default for RenderMetrics {
    fn default() -> Self {
        Self::TERMINAL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderContext {
    pub viewport: Size,
    pub metrics: RenderMetrics,
}

impl RenderContext {
    pub const fn terminal(viewport: Size) -> Self {
        Self {
            viewport,
            metrics: RenderMetrics::TERMINAL,
        }
    }

    pub const fn with_viewport(self, viewport: Size) -> Self {
        Self { viewport, ..self }
    }
}
