use std::sync::Arc;

use url::Url;

use crate::net::pool::FetchPool;
use crate::net::{FetchPayload, FetchRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Fetch,
    StartPage,
    Reject,
}

pub fn route(url: &Url) -> Route {
    match url.scheme() {
        "http" | "https" | "file" => Route::Fetch,
        "about" => match url.path() {
            "blank" => Route::StartPage,
            _ => Route::Reject,
        },
        _ => Route::Reject,
    }
}

pub trait Navigate: Send {
    fn submit(&self, tab_id: u64, generation: u64, url: Url);
    fn cancel(&self, _tab_id: u64, _generation: u64) {}
    fn poll_result(&self) -> Option<FetchPayload>;
}

pub struct PoolNet {
    pool: Arc<FetchPool>,
}

impl PoolNet {
    pub fn new(pool: Arc<FetchPool>) -> Self {
        Self { pool }
    }
}

impl Navigate for PoolNet {
    fn submit(&self, tab_id: u64, generation: u64, url: Url) {
        self.pool.submit(tab_id, generation, FetchRequest { url });
    }

    fn cancel(&self, tab_id: u64, generation: u64) {
        self.pool.cancel(tab_id, generation);
    }

    fn poll_result(&self) -> Option<FetchPayload> {
        self.pool.try_recv()
    }
}

pub struct NoopNet;

impl Navigate for NoopNet {
    fn submit(&self, _tab_id: u64, _generation: u64, _url: Url) {}

    fn poll_result(&self) -> Option<FetchPayload> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_schemes_fetch() {
        for url in [
            "http://example.com/",
            "https://example.com/",
            "file:///tmp/x.html",
        ] {
            assert_eq!(route(&Url::parse(url).expect("url")), Route::Fetch, "{url}");
        }
    }

    #[test]
    fn about_blank_is_the_start_page() {
        assert_eq!(
            route(&Url::parse("about:blank").expect("url")),
            Route::StartPage
        );
    }

    #[test]
    fn other_schemes_are_rejected() {
        for url in [
            "gopher://example.com/",
            "about:config",
            "ftp://example.com/",
        ] {
            assert_eq!(
                route(&Url::parse(url).expect("url")),
                Route::Reject,
                "{url}"
            );
        }
    }
}
