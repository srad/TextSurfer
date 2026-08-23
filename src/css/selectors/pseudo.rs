use std::fmt;

use cssparser::ToCss;
use selectors::parser::{NonTSPseudoClass, PseudoElement as PseudoElementTrait};

use crate::core::dom::NodeId;
use crate::core::style::PseudoElement;

use super::TextSurferSelectorImpl;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicPseudoClass {
    Link,
    AnyLink,
    Visited,
    Hover,
    Focus,
    Active,
    Checked,
    Enabled,
    Disabled,
}

impl DynamicPseudoClass {
    fn name(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::AnyLink => "any-link",
            Self::Visited => "visited",
            Self::Hover => "hover",
            Self::Focus => "focus",
            Self::Active => "active",
            Self::Checked => "checked",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    pub(super) fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "link" => Some(Self::Link),
            "any-link" => Some(Self::AnyLink),
            "visited" => Some(Self::Visited),
            "hover" => Some(Self::Hover),
            "focus" | "focus-visible" | "focus-within" => Some(Self::Focus),
            "active" => Some(Self::Active),
            "checked" => Some(Self::Checked),
            "enabled" => Some(Self::Enabled),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

impl ToCss for DynamicPseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        write!(dest, ":{}", self.name())
    }
}

impl NonTSPseudoClass for DynamicPseudoClass {
    type Impl = TextSurferSelectorImpl;

    fn is_active_or_hover(&self) -> bool {
        matches!(self, Self::Active | Self::Hover)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(self, Self::Active | Self::Hover | Self::Focus)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicState {
    pub hover: Option<NodeId>,
    pub focus: Option<NodeId>,
    pub active: Option<NodeId>,
}

impl DynamicState {
    pub const INERT: Self = Self {
        hover: None,
        focus: None,
        active: None,
    };
}

pub(super) fn pseudo_element_name(pseudo: PseudoElement) -> &'static str {
    match pseudo {
        PseudoElement::Before => "before",
        PseudoElement::After => "after",
        PseudoElement::Marker => "marker",
    }
}

pub(super) fn parse_pseudo_element_name(name: &str) -> Option<PseudoElement> {
    match name.to_ascii_lowercase().as_str() {
        "before" => Some(PseudoElement::Before),
        "after" => Some(PseudoElement::After),
        "marker" => Some(PseudoElement::Marker),
        _ => None,
    }
}

/// Newtype so the `selectors` crate's `PseudoElement` trait can be implemented for our own enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectorPseudoElement(pub PseudoElement);

impl ToCss for SelectorPseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        write!(dest, "::{}", pseudo_element_name(self.0))
    }
}

impl PseudoElementTrait for SelectorPseudoElement {
    type Impl = TextSurferSelectorImpl;
}
