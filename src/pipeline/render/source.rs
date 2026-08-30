use std::sync::Arc;

use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleSessionId {
    pub tab_id: u64,
    pub generation: u64,
    pub revision: u64,
}

#[derive(Clone)]
pub struct StyleInput {
    pub session: StyleSessionId,
    pub roots: Arc<[StyleSource]>,
}

#[derive(Clone)]
pub struct StyleSource {
    pub source: Option<Arc<str>>,
    pub base_url: Url,
    pub media: Arc<str>,
    pub imports: Arc<[StyleSource]>,
}
