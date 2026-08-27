mod document;
mod embedded;
mod response;

#[cfg(test)]
mod tests;

use crate::core::dom::SharedDocument;
use crate::core::style::StyleTree;
use crate::paint::DisplayList;

pub(crate) use document::render_document_with_images;

pub use document::{
    paint_document, render_document, render_html, render_html_with_context,
    render_html_with_text_rendering,
};
pub use embedded::embedded_style_sheets;
pub use response::response_kind;

pub enum ResponseKind {
    Html,
    PlainText,
    Unsupported(String),
}

pub struct RenderedPage {
    pub document: SharedDocument,
    pub styles: StyleTree,
    pub painted: DisplayList,
    pub parse_errors: usize,
    pub css_warnings: usize,
}
