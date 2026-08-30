use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusSource {
    Pointer,
    Keyboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusedNode {
    pub node: NodeId,
    pub source: FocusSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicState {
    pub hover: Option<NodeId>,
    pub focus: Option<FocusedNode>,
    pub active: Option<NodeId>,
}

impl DynamicState {
    pub const INERT: Self = Self {
        hover: None,
        focus: None,
        active: None,
    };
}
