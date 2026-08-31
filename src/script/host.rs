use crate::core::dom::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomQuery {
    DocumentElement,
    Body,
    ById(String),
    BySelector(String),
    Parent(NodeId),
    TextContent(NodeId),
    Attribute(NodeId, String),
    Title,
    Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomValue {
    Node(Option<NodeId>),
    Text(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutateOp {
    CreateElement(String),
    SetTextContent {
        node: NodeId,
        value: String,
    },
    SetAttribute {
        node: NodeId,
        name: String,
        value: String,
    },
    RemoveAttribute {
        node: NodeId,
        name: String,
    },
    AppendChild {
        parent: NodeId,
        child: NodeId,
    },
    InsertBefore {
        parent: NodeId,
        child: NodeId,
        sibling: Option<NodeId>,
    },
    Remove(NodeId),
    SetTitle(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutationImpact {
    pub render: bool,
    pub title: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutateResult {
    None,
    Node(NodeId),
    Removed(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKind {
    Push,
    Replace,
    Reload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEffect {
    Log(String),
    Alert(String),
    Navigate {
        target: String,
        kind: NavigationKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostOpId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostRequest {
    Fetch { url: String },
    Timer { delay_ms: u64, repeat: bool },
    CancelTimer { id: HostOpId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostResponse {
    pub status: u16,
    pub url: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCompletion {
    Fetch {
        id: HostOpId,
        result: Result<HostResponse, String>,
    },
    Timer {
        id: HostOpId,
    },
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostError {
    #[error("the document is busy")]
    BorrowConflict,
    #[error("invalid DOM operation: {0}")]
    Dom(String),
    #[error("host operation exceeded its limit")]
    Limit,
}

pub trait JsHost {
    fn query(&self, query: DomQuery) -> Result<DomValue, HostError>;
    fn mutate(&self, op: MutateOp) -> Result<MutateResult, HostError>;
    fn emit(&self, effect: HostEffect) -> Result<(), HostError>;
    fn request(&self, request: HostRequest) -> Result<HostOpId, HostError>;
}
