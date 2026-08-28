mod atom;
mod buckets;
mod dependencies;
mod element;
mod matching;
mod parser;
mod pseudo;

#[cfg(test)]
mod tests;

pub(crate) use buckets::{BucketKey, bucket_keys};
pub use dependencies::StateDeps;
pub(crate) use dependencies::uses_dynamic_state;
#[cfg(test)]
pub(crate) use matching::matching_specificity;
pub(crate) use matching::{MatchTarget, matching_specificity_with_forms};
pub(super) use parser::TextSurferSelectorImpl;
pub(crate) use parser::{ParsedSelectors, parse};
pub use pseudo::{DynamicState, FocusSource, FocusedNode};
