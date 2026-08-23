mod atom;
mod buckets;
mod element;
mod matching;
mod parser;
mod pseudo;

#[cfg(test)]
mod tests;

pub(crate) use buckets::{BucketKey, bucket_keys};
pub(crate) use matching::{MatchTarget, matching_specificity};
pub(super) use parser::TextSurferSelectorImpl;
pub(crate) use parser::{ParsedSelectors, parse};
pub(crate) use pseudo::DynamicState;
