use selectors::parser::Component;

use super::ParsedSelectors;

/// The one component a rule can be pre-filtered on. Bucketing turns "match every rule against every
/// element" into "match only the rules that name this element's id, class, tag or nothing at all".
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum BucketKey {
    Id(String),
    Class(String),
    LocalName(String),
    Universal,
}

pub(crate) fn bucket_keys(selectors: &ParsedSelectors, quirks: bool) -> Vec<BucketKey> {
    let mut keys = Vec::new();
    for selector in selectors.slice() {
        let mut id = None;
        let mut class = None;
        let mut local_names = Vec::new();
        for component in selector.iter() {
            match component {
                Component::ID(value) if id.is_none() => {
                    id = Some(normalize_bucket(value.as_str(), quirks));
                }
                Component::Class(value) if class.is_none() => {
                    class = Some(normalize_bucket(value.as_str(), quirks));
                }
                Component::LocalName(value) if local_names.is_empty() => {
                    local_names.push(value.name.as_str().to_string());
                    if value.lower_name != value.name {
                        local_names.push(value.lower_name.as_str().to_string());
                    }
                }
                _ => {}
            }
        }
        if let Some(id) = id {
            keys.push(BucketKey::Id(id));
        } else if let Some(class) = class {
            keys.push(BucketKey::Class(class));
        } else if !local_names.is_empty() {
            keys.extend(local_names.into_iter().map(BucketKey::LocalName));
        } else {
            keys.push(BucketKey::Universal);
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

fn normalize_bucket(value: &str, quirks: bool) -> String {
    if quirks {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}
