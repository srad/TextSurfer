mod document;
#[cfg(test)]
mod embedded;
mod queue;
mod response;
mod source;

#[cfg(test)]
mod tests;

use crate::core::dom::SharedDocument;
use crate::core::style::StyleTree;
use crate::paint::DisplayList;
use std::sync::Arc;

pub use document::{
    paint_document, render_document, render_html, render_html_with_context,
    render_html_with_text_rendering,
};
#[cfg(test)]
pub use embedded::embedded_style_sheets;
pub(crate) use queue::RenderCause;
pub use queue::{
    BlockingRenderQueue, RenderActivity, RenderCauses, RenderJob, RenderKey, RenderPoll,
    RenderQueue, RenderResult, RenderStage, RenderSubmitted, RenderTimings, RenderTree, RenderWork,
    ThreadedRenderQueue,
};
pub use response::response_kind;
pub use source::{StyleInput, StyleSessionId, StyleSource};

pub enum ResponseKind {
    Html,
    PlainText,
    Unsupported(String),
}

pub struct RenderedPage {
    pub document: SharedDocument,
    pub styles: Arc<StyleTree>,
    pub painted: DisplayList,
    pub painted_changed: bool,
    pub parse_errors: usize,
    pub css_warnings: usize,
}
