use std::collections::HashMap;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    None,
    #[default]
    Inline,
    Block,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Pre,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeSizes {
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
    pub left: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyle {
    pub display: Display,
    pub white_space: WhiteSpace,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleTree {
    styles: HashMap<NodeId, ComputedStyle>,
}

impl StyleTree {
    pub fn insert(&mut self, node: NodeId, style: ComputedStyle) {
        self.styles.insert(node, style);
    }

    pub fn get(&self, node: NodeId) -> ComputedStyle {
        self.styles.get(&node).copied().unwrap_or_default()
    }
}
