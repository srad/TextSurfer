//! Replaced elements the terminal cannot render, reduced to their text fallback.

use crate::core::dom::{Attr, AttrNs};

pub(super) fn image_fallback(attrs: &[Attr]) -> Option<String> {
    let alt = attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == "alt");
    match alt {
        Some(attr) => {
            let value = attr.value.trim();
            (!value.is_empty()).then(|| format!("[{value}]"))
        }
        None => Some("[img]".to_string()),
    }
}
