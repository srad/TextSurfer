use std::sync::Arc;

use url::Url;

use crate::net::pool::FetchPool;
use crate::net::{FetchPoll, FetchRequest, ResourceId, Submitted};

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
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) -> Submitted;
    fn cancel(&self, _tab_id: u64, _generation: u64) {}
    fn poll_result(&self) -> FetchPoll;

    /// Stop accepting work and return without joining. Quitting must not wait on a
    /// worker parked inside the 30 s fetch timeout, so the frontends call this before
    /// the pool's own `Drop` — which joins — can run.
    fn shutdown(&self) {}
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
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) -> Submitted {
        self.pool
            .submit(tab_id, generation, resource_id, FetchRequest { url })
    }

    fn cancel(&self, tab_id: u64, generation: u64) {
        self.pool.cancel(tab_id, generation);
    }

    fn poll_result(&self) -> FetchPoll {
        self.pool.try_recv()
    }

    fn shutdown(&self) {
        self.pool.shutdown_without_waiting();
    }
}

pub struct NoopNet;

impl Navigate for NoopNet {
    fn submit(
        &self,
        _tab_id: u64,
        _generation: u64,
        _resource_id: ResourceId,
        _url: Url,
    ) -> Submitted {
        // `Queued`, not `Closed`: the no-op net accepts the job and simply never
        // completes it. Reporting `Closed` would make every navigation in an
        // I/O-free composition look like a failed load.
        Submitted::Queued
    }

    fn poll_result(&self) -> FetchPoll {
        FetchPoll::Empty
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
