use crate::core::dom::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutateOp {
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsEvent {
    Click { target: NodeId },
}

pub trait JsHost: Send {
    fn apply_mutation(&mut self, op: MutateOp);
    fn log(&mut self, message: &str);
}
