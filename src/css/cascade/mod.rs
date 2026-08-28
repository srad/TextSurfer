mod alignment;
mod content;
mod counters;
mod declaration;
mod document;
mod dynamic;
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
pub(in crate::css) use declaration::is_supported_property;
pub(crate) use document::CascadeState;
pub(in crate::css) use media::active_style_rules;
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

impl BasicCascade {
    pub(crate) fn apply_retained_with_form_state(
        &self,
        sheets: &[StyleSheet],
        document: &Document,
        media: MediaContext,
        forms: &FormState,
    ) -> (StyleTree, CascadeState) {
        let output = document::cascade_document_retained(sheets, document, media, forms, true);
        (output.tree, output.state)
    }

    pub(crate) fn apply_dynamic_with_form_state(
        &self,
        sheets: &[StyleSheet],
        document: &Document,
        media: MediaContext,
        forms: &FormState,
        previous: &StyleTree,
        state: &CascadeState,
    ) -> Option<(StyleTree, CascadeState)> {
        dynamic::cascade_dynamic(sheets, document, media, forms, previous, state)
    }
}

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
