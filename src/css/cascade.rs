use crate::core::dom::Document;
use crate::core::style::StyleTree;
use crate::css::StyleSheet;

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document) -> StyleTree;
}
