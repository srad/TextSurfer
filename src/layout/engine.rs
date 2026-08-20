use crate::core::dom::Document;
use crate::core::geom::Size;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxTree;

pub trait LayoutEngine: Send + Sync {
    fn layout(&self, document: &Document, viewport: Size) -> BoxTree;
}
