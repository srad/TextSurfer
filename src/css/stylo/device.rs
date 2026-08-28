use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use euclid::{Scale, Size2D};
use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::device::Device;
use style::device::servo::FontMetricsProvider;
use style::font_metrics::FontMetrics;
use style::media_queries::MediaType;
use style::properties::{ComputedValues, style_structs};
use style::queries::values::PrefersColorScheme;
use style::servo::media_features::PointerCapabilities;
use style::values::computed::font::{GenericFontFamily, QueryFontMetricsFlags};
use style::values::computed::{CSSPixelLength, Length};
use style_traits::{CSSPixel, DevicePixel};

use crate::core::geom::Size;
use crate::core::style::{CellMetric, LengthAxis};

/// Font metrics for a character cell.
///
/// Every optional metric is deliberately `None`. Stylo's spec fallbacks are `x_height → 0.5em` and,
/// for a non-upright writing mode, `zero_advance_measure → 0.5em` — which is exactly the locked
/// terminal rule in `CellMetric::css_pixels_with_fonts` (`Ex | Ch => font_px / 2.0`). Returning our
/// own constant here would duplicate that rule rather than honour it.
///
/// `cap_height` and `ic_width` stay `None` for a different reason: `cap`, `rcap`, `ic` and `ric` are
/// not in `CssLengthUnit` at all, so the custom cascade rejects them outright. Stylo will accept
/// them; that divergence is recorded against M7 rather than papered over here.
#[derive(Debug)]
pub(super) struct CellFontMetrics {
    metric: CellMetric,
    queries: Arc<AtomicUsize>,
}

impl CellFontMetrics {
    pub(super) fn new(metric: CellMetric) -> Self {
        Self {
            metric,
            queries: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A handle to the query count, cloneable before the provider is boxed into the device.
    ///
    /// Stylo's spec fallback for `ex`/`ch` is 0.5em, which is also the terminal's locked rule, so a
    /// value assertion alone cannot tell a real answer from the fallback. This counter can.
    pub(super) fn counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.queries)
    }
}

impl FontMetricsProvider for CellFontMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &style_structs::Font,
        base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> FontMetrics {
        self.queries.fetch_add(1, Ordering::Relaxed);
        FontMetrics {
            // Nothing consumes this: it reaches only `font-size-adjust: from-font`, which the
            // terminal does not support.
            ascent: base_size * 0.8,
            ..Default::default()
        }
    }

    fn base_size_for_generic(&self, _generic: GenericFontFamily) -> Length {
        // One font, so the family cannot change the size.
        Length::new(f32::from(self.metric.root_font_px()))
    }
}

/// Build a Stylo [`Device`] from the injected cell metric and viewport.
///
/// The viewport is [`CellMetric::viewport_css_pixels`] on both axes — the same function the
/// media-query path already uses, which is what keeps "media queries compare unrounded CSS pixels"
/// true for the VGA and terminal metrics alike.
pub(super) fn device(metric: CellMetric, viewport: Size, quirks_mode: QuirksMode) -> Device {
    device_with_metrics(
        metric,
        viewport,
        quirks_mode,
        Box::new(CellFontMetrics::new(metric)),
    )
}

pub(super) fn device_with_metrics(
    metric: CellMetric,
    viewport: Size,
    quirks_mode: QuirksMode,
    metrics: Box<dyn FontMetricsProvider>,
) -> Device {
    let width = metric.viewport_css_pixels(LengthAxis::Horizontal, viewport) as f32;
    let height = metric.viewport_css_pixels(LengthAxis::Vertical, viewport) as f32;
    let size = Size2D::<f32, CSSPixel>::new(width, height);
    Device::new(
        MediaType::screen(),
        quirks_mode,
        size,
        Size2D::<f32, DevicePixel>::new(width, height),
        Scale::new(1.0),
        metrics,
        default_computed_values(),
        // S5 wires `MediaContext` — palette, colour scheme and pointer capabilities — into the
        // device. Until then these are placeholders that make the device constructible.
        PrefersColorScheme::Dark,
        PointerCapabilities::default(),
        PointerCapabilities::default(),
    )
}

fn default_computed_values() -> ServoArc<ComputedValues> {
    let mut font = style_structs::Font::initial_values();
    // The generated constructor leaves the cache key at zero.
    font.compute_font_hash();
    ComputedValues::initial_values_with_font_override(font)
}
