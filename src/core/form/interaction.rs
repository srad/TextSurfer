use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    ControlKind, ControlValue, FormState, control_kind, form_owner, is_disabled, max_length,
    options,
};
use crate::core::dom::{Document, Node, NodeId, attr_value, has_attr};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FormMutationError {
    #[error("the control cannot be edited")]
    Inert,
}

impl FormState {
    pub fn set_text(
        &mut self,
        document: &Document,
        node: NodeId,
        mut value: String,
    ) -> Result<(), FormMutationError> {
        let kind = control_kind(document, node).ok_or(FormMutationError::Inert)?;
        if !kind.is_text_entry() || is_disabled(document, node) {
            return Err(FormMutationError::Inert);
        }
        let attrs = match document.node(node) {
            Some(Node::Element { attrs, .. }) => attrs,
            _ => return Err(FormMutationError::Inert),
        };
        if has_attr(attrs, "readonly") {
            return Err(FormMutationError::Inert);
        }
        if kind != ControlKind::TextArea {
            value.retain(|character| !matches!(character, '\r' | '\n'));
        }
        if let Some(limit) = max_length(attrs) {
            value = value.graphemes(true).take(limit).collect();
        }
        self.set(node, ControlValue::Text(value));
        Ok(())
    }

    pub fn set_checked(
        &mut self,
        document: &Document,
        node: NodeId,
        checked: bool,
    ) -> Result<(), FormMutationError> {
        let kind = control_kind(document, node).ok_or(FormMutationError::Inert)?;
        if !kind.is_toggle() || is_disabled(document, node) {
            return Err(FormMutationError::Inert);
        }
        if kind == ControlKind::Radio && checked {
            let name = match document.node(node) {
                Some(Node::Element { attrs, .. }) => attr_value(attrs, "name").unwrap_or(""),
                _ => return Err(FormMutationError::Inert),
            };
            if !name.is_empty() {
                let owner = form_owner(document, node);
                for candidate in tree_order(document) {
                    if candidate != node
                        && control_kind(document, candidate) == Some(ControlKind::Radio)
                        && form_owner(document, candidate) == owner
                        && matches!(document.node(candidate), Some(Node::Element { attrs, .. })
                            if attr_value(attrs, "name").unwrap_or("") == name)
                    {
                        self.set(candidate, ControlValue::Checked(false));
                    }
                }
            }
        }
        self.set(node, ControlValue::Checked(checked));
        Ok(())
    }

    pub fn select(
        &mut self,
        document: &Document,
        node: NodeId,
        index: usize,
    ) -> Result<(), FormMutationError> {
        if control_kind(document, node) != Some(ControlKind::Select) || is_disabled(document, node)
        {
            return Err(FormMutationError::Inert);
        }
        let Some(Node::Element { attrs, .. }) = document.node(node) else {
            return Err(FormMutationError::Inert);
        };
        if has_attr(attrs, "multiple") {
            return Err(FormMutationError::Inert);
        }
        let choices = options(document, node);
        let selected = (0..choices.len())
            .map(|offset| (index + offset) % choices.len())
            .find(|candidate| !is_disabled(document, choices[*candidate]))
            .ok_or(FormMutationError::Inert)?;
        self.set(node, ControlValue::Selected(selected));
        Ok(())
    }

    pub fn reset_form(&mut self, document: &Document, form: NodeId) {
        self.overrides
            .retain(|node, _| form_owner(document, *node) != Some(form));
    }
}

fn tree_order(document: &Document) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
    while let Some(node) = stack.pop() {
        result.push(node);
        stack.extend(document.children(node).into_iter().rev());
    }
    result
}
