use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use textsurf::app::App;
use textsurf::app::net::Navigate;
use textsurf::net::{Fetch, FetchError, FetchPayload, FetchRequest, FetchResponse, SchemeFetch};

const BODY: &str = "<!doctype html><title>smoke</title><p>acceptance</p>";

struct FixtureFetch;

impl Fetch for FixtureFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        if request.url.path() == "/gone" {
            return Err(FetchError::HttpStatus(410));
        }
        Ok(FetchResponse {
            final_url: request.url.clone(),
            body: BODY.as_bytes().to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        })
    }
}

struct ImmediateNet {
    fetch: Arc<dyn Fetch>,
    pending: Mutex<VecDeque<FetchPayload>>,
}

impl Navigate for ImmediateNet {
    fn submit(&self, tab_id: u64, generation: u64, url: url::Url) {
        let result = self.fetch.fetch(&FetchRequest { url });
        self.pending
            .lock()
            .expect("immediate result lock")
            .push_back(FetchPayload {
                tab_id,
                generation,
                result,
            });
    }

    fn poll_result(&self) -> Option<FetchPayload> {
        self.pending
            .lock()
            .expect("immediate result lock")
            .pop_front()
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
            .lines
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
            .lines
            .iter()
            .any(|line| line.contains("acceptance"))
    );
    assert!(app.message().contains("accepted gen 1"));
}

#[test]
fn composed_pipeline_renders_fetch_errors_as_a_page() {
    let mut app = app_with_fixture_fetch();
    app.submit_url("https://example.com/gone");
    deliver(&mut app);
    assert!(
        app.chrome_view()
            .content
            .lines
            .first()
            .is_some_and(|line| line.starts_with("failed to load"))
    );
    assert!(app.message().contains("http status 410"));
}
