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
    pub body: Vec<u8>,
    pub content_type: Option<String>,
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
