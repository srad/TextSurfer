mod content;
mod counters;
mod declaration;
mod document;
mod flex;
mod media;
mod typography;

#[cfg(test)]
mod tests;

use crate::core::dom::Document;
use crate::core::style::StyleTree;
use crate::css::StyleSheet;

pub use counters::LIST_ITEM_COUNTER;
pub use media::{MediaContext, media_query_list_matches};

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree;
}

#[derive(Default)]
pub struct BasicCascade;

impl Cascade for BasicCascade {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
        document::cascade_document(sheets, document, media, true)
    }
}
