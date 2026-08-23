pub mod engine;
mod table;
mod text_flow;

use crate::core::dom::{Attr, AttrNs};

fn image_fallback(attrs: &[Attr]) -> Option<String> {
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

pub use engine::{
    BackgroundFill, BorderStroke, BoxTree, LayoutBox, LayoutEngine, LayoutRect, LinkBox,
    TaffyLayoutEngine, TextFragment,
};
