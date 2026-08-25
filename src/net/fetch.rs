use std::sync::Arc;

use thiserror::Error;
use url::Url;

pub const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u64);

impl ResourceId {
    pub const DOCUMENT: Self = Self(0);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchResponse {
    pub final_url: Url,
    /// The HTTP status the response arrived with. Non-HTTP fetchers report `200`:
    /// they either produced a body or failed outright.
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

impl FetchResponse {
    /// A 4xx/5xx page is still a page. Its body is rendered, but it is never treated
    /// as content a subresource can use — a 404 is not a stylesheet.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("http status {0}")]
    HttpStatus(u16),
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
    #[error("response body exceeds {limit} bytes")]
    BodyTooLarge { limit: usize },
}

pub trait Fetch: Send + Sync {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError>;
}

pub struct SchemeFetch {
    pub http: Arc<dyn Fetch>,
    pub file: Arc<dyn Fetch>,
}

impl Fetch for SchemeFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        match request.url.scheme() {
            "http" | "https" => self.http.fetch(request),
            "file" => self.file.fetch(request),
            other => Err(FetchError::UnsupportedScheme(other.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchPayload {
    pub tab_id: u64,
    pub generation: u64,
    pub resource_id: ResourceId,
    pub result: Result<FetchResponse, FetchError>,
}

/// What happened to a job handed to the pool. `submit` used to return `()`, so a
/// job the pool refused — a duplicate key, or a pool already shut down — vanished
/// and the caller waited forever for a result that would never arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submitted {
    /// The job is queued; a payload will arrive unless it is canceled.
    Queued,
    /// `(tab, generation, resource)` is already in flight, so this job was coalesced
    /// into the live one. The live job's payload is the only one that will arrive.
    Duplicate,
    /// The pool is shut down and accepted nothing. No payload will ever arrive.
    Closed,
}

/// The result of asking the pool for a finished job. `try_recv` used to collapse
/// "nothing yet" and "every worker is gone" into `None`, so a dead pool looked
/// exactly like an idle one and pending loads hung silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchPoll {
    /// A finished job.
    Ready(FetchPayload),
    /// Nothing finished yet; ask again later.
    Empty,
    /// Every sender is gone, so no further payload can ever arrive.
    Disconnected,
}
