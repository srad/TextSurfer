pub mod encoding;
pub mod fetch;
pub mod file;
pub mod http;
pub mod pool;

pub use encoding::{Decoded, charset_from_content_type, decode, decode_text};
pub use fetch::{
    Fetch, FetchError, FetchPayload, FetchPoll, FetchRequest, FetchResponse, MAX_BODY_BYTES,
    ResourceId, SchemeFetch, Submitted,
};
pub use file::FileFetch;
pub use http::{UreqFetch, default_user_agent};
pub use pool::FetchPool;
