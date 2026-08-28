mod alignment;
mod content;
mod counters;
mod declaration;
mod document;
mod flex;
mod grid;
mod media;
mod typography;

#[cfg(test)]
mod tests;

use crate::core::dom::Document;
use crate::core::form::FormState;
use crate::core::style::StyleTree;
use crate::css::StyleSheet;

pub use counters::LIST_ITEM_COUNTER;
pub use media::{MediaContext, media_query_list_matches};

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
        self.apply_with_form_state(sheets, document, media, FormState::empty())
    }

    fn apply_with_form_state(
        &self,
        sheets: &[StyleSheet],
        document: &Document,
        media: MediaContext,
        forms: &FormState,
    ) -> StyleTree;
}

#[derive(Default)]
pub struct BasicCascade;

impl Cascade for BasicCascade {
    fn apply_with_form_state(
        &self,
        sheets: &[StyleSheet],
        document: &Document,
        media: MediaContext,
        forms: &FormState,
    ) -> StyleTree {
        document::cascade_document(sheets, document, media, forms, true)
    }
}
