pub(crate) mod counters;
mod media;

#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::core::dom::Document;
#[cfg(test)]
use crate::core::form::FormState;
#[cfg(test)]
use crate::core::style::StyleTree;
#[cfg(test)]
use crate::css::StyleSheet;

pub use counters::LIST_ITEM_COUNTER;
pub use media::MediaContext;

#[cfg(test)]
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

#[cfg(test)]
#[derive(Default)]
pub struct StyloCascade;

#[cfg(test)]
impl Cascade for StyloCascade {
    fn apply_with_form_state(
        &self,
        sheets: &[StyleSheet],
        document: &Document,
        media: MediaContext,
        forms: &FormState,
    ) -> StyleTree {
        let author_css = sheets
            .iter()
            .map(|sheet| sheet.source.as_str())
            .collect::<Vec<_>>();
        crate::css::stylo::cascade_once(document, forms, media, &author_css).0
    }
}
