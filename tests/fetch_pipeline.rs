use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use textsurfer::app::App;
use textsurfer::app::net::Navigate;
use textsurfer::net::{
    Fetch, FetchError, FetchPayload, FetchPoll, FetchRequest, FetchResponse, ResourceId,
    SchemeFetch, Submitted,
};

const BODY: &str = "<!doctype html><title>smoke</title><p>acceptance</p>";

struct FixtureFetch;

impl Fetch for FixtureFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        // A gone page that sends its own HTML, the way a real server does, and a
        // gone page that sends nothing at all.
        if request.url.path() == "/gone" {
            return Ok(FetchResponse {
                final_url: request.url.clone(),
                status: 410,
                body: b"<h1>This page is gone</h1>".to_vec(),
                content_type: Some("text/html; charset=utf-8".to_string()),
            });
        }
        if request.url.path() == "/gone-silently" {
            return Ok(FetchResponse {
                final_url: request.url.clone(),
                status: 410,
                body: Vec::new(),
                content_type: None,
            });
        }
        if request.url.path() == "/unreachable" {
            return Err(FetchError::Network("name not resolved".to_string()));
        }
        Ok(FetchResponse {
            final_url: request.url.clone(),
            status: 200,
            body: BODY.as_bytes().to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        })
    }
}

struct ExternalStyleFetch;

impl Fetch for ExternalStyleFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let (body, content_type) = if request.url.path().ends_with("site.css") {
            (b"#navigation { display: none }".to_vec(), "text/css")
        } else {
            (
                b"<!doctype html><link rel=stylesheet href='/site.css'>\
                  <nav id=navigation>sidebar</nav><main>article</main>"
                    .to_vec(),
                "text/html; charset=utf-8",
            )
        };
        Ok(FetchResponse {
            final_url: request.url.clone(),
            status: 200,
            body,
            content_type: Some(content_type.to_string()),
        })
    }
}

struct ImmediateNet {
    fetch: Arc<dyn Fetch>,
    pending: Mutex<VecDeque<FetchPayload>>,
}

impl Navigate for ImmediateNet {
    fn submit(
        &self,
        tab_id: u64,
        generation: u64,
        resource_id: ResourceId,
        url: url::Url,
    ) -> Submitted {
        let result = self.fetch.fetch(&FetchRequest { url });
        self.pending
            .lock()
            .expect("immediate result lock")
            .push_back(FetchPayload {
                tab_id,
                generation,
                resource_id,
                result,
            });
        Submitted::Queued
    }

    fn poll_result(&self) -> FetchPoll {
        self.pending
            .lock()
            .expect("immediate result lock")
            .pop_front()
            .map_or(FetchPoll::Empty, FetchPoll::Ready)
    }
}

fn app_with_fixture_fetch() -> App {
    let fixture: Arc<dyn Fetch> = Arc::new(FixtureFetch);
    let fetch = Arc::new(SchemeFetch {
        http: Arc::clone(&fixture),
        file: fixture,
    });
    let net: Arc<dyn Navigate> = Arc::new(ImmediateNet {
        fetch,
        pending: Mutex::new(VecDeque::new()),
    });
    App::with_net(net)
}

fn deliver(app: &mut App) {
    app.step(Duration::ZERO);
}

#[test]
fn composed_pipeline_loads_and_renders_http_html() {
    let mut app = app_with_fixture_fetch();
    app.submit_url("https://example.com/");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("acceptance"))
    );
    assert!(app.message().contains("accepted gen 1"));
}

#[test]
fn composed_pipeline_routes_file_urls_through_the_file_fetch_boundary() {
    let mut app = app_with_fixture_fetch();
    app.submit_url("file:///fixture.html");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("acceptance"))
    );
    assert!(app.message().contains("accepted gen 1"));
}

#[test]
fn composed_pipeline_renders_the_servers_own_error_page() {
    // This used to assert a "failed to load" stub, because the 410's body was thrown
    // away before it reached the renderer. A browser shows the page the server sent.
    let mut app = app_with_fixture_fetch();
    app.submit_url("https://example.com/gone");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("This page is gone")),
        "the server's own body renders, got {:?}",
        app.chrome_view().content.painted.text_lines()
    );
    assert!(
        app.message().contains("HTTP 410"),
        "and the reader is still told it is an error, got {:?}",
        app.message()
    );
}

#[test]
fn composed_pipeline_reports_an_error_status_with_no_body() {
    let mut app = app_with_fixture_fetch();
    app.submit_url("https://example.com/gone-silently");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .painted
            .text_lines()
            .first()
            .is_some_and(|line| line.starts_with("failed to load")),
        "nothing renderable arrived, so the reader gets an explanation"
    );
    assert!(
        app.message().contains("HTTP 410"),
        "got {:?}",
        app.message()
    );
}

#[test]
fn composed_pipeline_reports_a_transport_failure() {
    let mut app = app_with_fixture_fetch();
    app.submit_url("https://example.com/unreachable");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .painted
            .text_lines()
            .first()
            .is_some_and(|line| line.starts_with("failed to load"))
    );
    assert!(
        app.message().contains("name not resolved"),
        "got {:?}",
        app.message()
    );
}

#[test]
fn composed_pipeline_applies_external_author_styles() {
    let fixture: Arc<dyn Fetch> = Arc::new(ExternalStyleFetch);
    let fetch = Arc::new(SchemeFetch {
        http: Arc::clone(&fixture),
        file: fixture,
    });
    let net: Arc<dyn Navigate> = Arc::new(ImmediateNet {
        fetch,
        pending: Mutex::new(VecDeque::new()),
    });
    let mut app = App::with_net(net);
    app.submit_url("https://example.com/article");
    deliver(&mut app);
    let lines = app.chrome_view().content.painted.text_lines();
    assert!(lines.iter().any(|line| line.contains("article")));
    assert!(!lines.iter().any(|line| line.contains("sidebar")));
    assert!(app.message().contains("1 stylesheets"));
}
