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
    FocusVisible,
    FocusWithin,
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
            Self::FocusVisible => "focus-visible",
            Self::FocusWithin => "focus-within",
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
            "focus" => Some(Self::Focus),
            "focus-visible" => Some(Self::FocusVisible),
            "focus-within" => Some(Self::FocusWithin),
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
        matches!(
            self,
            Self::Active | Self::Hover | Self::Focus | Self::FocusVisible | Self::FocusWithin
        )
    }
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

    fn accepts_state_pseudo_classes(&self) -> bool {
        true
    }
}
