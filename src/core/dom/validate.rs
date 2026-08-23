use thiserror::Error;

use super::{Document, NodeId};

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DomError {
    #[error("node does not exist")]
    InvalidNode,
    #[error("mutation would create a cycle")]
    Cycle,
    #[error("a node cannot be inserted relative to itself")]
    SameNode,
}

impl Document {
    pub(super) fn ensure_node(&self, id: NodeId) -> Result<(), DomError> {
        if self.arena.get(id.0).is_some() {
            Ok(())
        } else {
            Err(DomError::InvalidNode)
        }
    }

    pub(super) fn validate_move(&self, id: NodeId, parent: Option<NodeId>) -> Result<(), DomError> {
        self.ensure_node(id)?;
        if let Some(parent) = parent {
            self.ensure_node(parent)?;
            if parent
                .0
                .ancestors(&self.arena)
                .any(|ancestor| ancestor == id.0)
            {
                return Err(DomError::Cycle);
            }
        }
        Ok(())
    }
}
