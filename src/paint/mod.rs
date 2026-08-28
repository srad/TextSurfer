pub mod painter;

pub use painter::{
    BasicPainter, DisplayList, HitKind, HitRegion, PaintOverlay, PaintedImage, PaintedLink,
    PaintedRow, PaintedSpan, Painter, ScaledTextRun, legible_foreground, resolve_cell_style,
};
