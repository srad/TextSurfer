use crate::core::dom::{Document, SharedDocument};
use crate::core::geom::Size;
use crate::core::style::{Palette, RenderContext, RenderMetrics, StyleTree, TextRendering};
use crate::css::{BasicCascade, Cascade, ColorScheme, MediaContext, StyleSheet};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};
use crate::paint::{BasicPainter, DisplayList, Painter};
use crate::pipeline::page_load::{PageLoad, PageLoadOptions};

use super::RenderedPage;

pub fn render_html(
    source: &str,
    viewport: Size,
    palette: Palette,
    scripting: bool,
) -> RenderedPage {
    render_html_with_context(
        source,
        RenderContext::terminal(viewport),
        palette,
        scripting,
    )
}

pub fn render_html_with_text_rendering(
    source: &str,
    viewport: Size,
    palette: Palette,
    scripting: bool,
    text_rendering: TextRendering,
) -> RenderedPage {
    render_html_with_context(
        source,
        RenderContext {
            viewport,
            metrics: RenderMetrics {
                cell: crate::core::style::CellMetric::DEFAULT,
                text: text_rendering,
            },
        },
        palette,
        scripting,
    )
}

pub fn render_html_with_context(
    source: &str,
    render: RenderContext,
    palette: Palette,
    scripting: bool,
) -> RenderedPage {
    let base = url::Url::parse("about:blank").expect("the static synthetic base parses");
    let mut load = PageLoad::new(
        source,
        base,
        encoding_rs::UTF_8,
        PageLoadOptions {
            render,
            palette,
            scripting,
            color_scheme: ColorScheme::Dark,
            started: std::time::Duration::ZERO,
        },
    );
    load.force_render()
}

pub fn render_document(
    document: SharedDocument,
    sheets: &[StyleSheet],
    media: MediaContext,
    palette: Palette,
    parse_errors: usize,
) -> RenderedPage {
    let css_warnings = sheets
        .iter()
        .map(|sheet| sheet.diagnostics.total())
        .sum::<usize>();
    let styles = BasicCascade.apply(sheets, &document.borrow(), media);
    let painted = paint_document(&document.borrow(), &styles, media.viewport, palette);
    RenderedPage {
        document,
        styles,
        painted,
        parse_errors,
        css_warnings,
    }
}

pub fn paint_document(
    document: &Document,
    styles: &StyleTree,
    viewport: Size,
    palette: Palette,
) -> DisplayList {
    let boxes = TaffyLayoutEngine.layout(document, styles, viewport);
    BasicPainter.paint(&boxes, palette)
}
