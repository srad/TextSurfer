use super::*;

fn scrolled_page_waiting_for_late_stylesheet() -> (App, FetchPayload) {
    let fake = Arc::new(FakeNet::default());
    let mut app = App::with_net(fake.clone());
    app.on_resize(Size { cols: 40, rows: 8 });
    app.submit_url("https://example.com/page");
    let document = fake.pending.lock().unwrap().pop().unwrap();
    let paragraphs = "<p>row</p>".repeat(50);
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/page").unwrap(),
            body: format!(
                "<!doctype html><html><head><link rel=stylesheet href=late.css></head><body>{paragraphs}</body></html>"
            )
            .into_bytes(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
        ..document
    }));
    let stylesheet = fake.pending.lock().unwrap().pop().unwrap();
    app.step(STYLESHEET_DEADLINE);
    app.handle_key(press(Key::End));
    assert!(app.tabs.active().scroll > 0);
    (app, stylesheet)
}

fn hide_all_paragraphs(payload: FetchPayload) -> FetchPayload {
    let final_url = payload.result.as_ref().unwrap().final_url.clone();
    FetchPayload {
        result: Ok(FetchResponse {
            final_url,
            body: b"p { display: none }".to_vec(),
            content_type: Some("text/css".to_string()),
        }),
        ..payload
    }
}

#[test]
fn stale_fetch_results_are_dropped_fresh_ones_accepted() {
    let mut app = App::new();
    app.submit_url("https://example.com");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    let message = app.message().to_string();
    assert!(!app.deliver_fetch(FetchPayload {
        tab_id,
        generation: generation - 1,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/").unwrap(),
            body: vec![],
            content_type: None,
        }),
    }));
    assert_eq!(app.message(), message);
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/").unwrap(),
            body: vec![],
            content_type: None,
        }),
    }));
    assert!(app.message().contains("accepted"));
}

#[test]
fn a_background_tabs_current_fetch_is_delivered_to_that_tab() {
    let mut app = App::new();
    app.submit_url("https://a.example");
    let background_tab_id = app.tabs.active().id;
    let background_generation = app.tabs.active().generation;
    app.new_tab();
    assert!(app.deliver_fetch(FetchPayload {
        tab_id: background_tab_id,
        generation: background_generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://a.example/final").unwrap(),
            body: b"<p>background complete</p>".to_vec(),
            content_type: Some("text/html; charset=\"utf-8\"".to_string()),
        }),
    }));
    assert_eq!(app.tabs.active_index(), 1);
    assert_eq!(app.tabs.tabs()[0].url, "https://a.example/final");
    assert!(
        !app.tabs.tabs()[0]
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("background complete"))
    );
    assert_eq!(app.message(), STARTUP_HINT);
    app.apply(Action::PrevTab);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("background complete"))
    );
    assert!(app.message().contains("accepted gen"));
}

#[test]
fn late_stylesheet_repaint_clamps_active_tab_scroll() {
    let (mut app, stylesheet) = scrolled_page_waiting_for_late_stylesheet();
    assert!(app.deliver_fetch(hide_all_paragraphs(stylesheet)));
    assert_eq!(app.tabs.active().painted.len(), 0);
    assert_eq!(app.tabs.active().scroll, 0);
}

#[test]
fn late_stylesheet_repaint_clamps_on_background_tab_activation() {
    let (mut app, stylesheet) = scrolled_page_waiting_for_late_stylesheet();
    let painted_rows = app.tabs.active().painted.len();
    let scroll = app.tabs.active().scroll;
    app.new_tab();
    assert!(app.deliver_fetch(hide_all_paragraphs(stylesheet)));
    assert_eq!(app.tabs.tabs()[0].painted.len(), painted_rows);
    assert_eq!(app.tabs.tabs()[0].scroll, scroll);
    app.apply(Action::PrevTab);
    assert_eq!(app.tabs.active().painted.len(), 0);
    assert_eq!(app.tabs.active().scroll, 0);
}

#[test]
fn fake_fetch_loads_the_rendered_document_into_the_tab() {
    let fake: Arc<dyn Navigate> = Arc::new(FakeNet::default());
    let mut app = App::with_net(fake);
    app.submit_url("example.com");
    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.active_url(), "https://example.com");
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.starts_with("  fetching"))
    );
    app.step(Duration::ZERO);
    let tab = app.tabs.active();
    assert_eq!(
        tab.painted.text_lines().first().map(String::as_str),
        Some("hi there")
    );
    assert_eq!(tab.title, "https://example.com/");
    assert!(
        app.message()
            .contains("accepted gen 1 - https://example.com/")
    );
}

#[test]
fn fetch_errors_land_in_the_tab_content() {
    struct ErrorNet {
        pending: Mutex<Vec<FetchPayload>>,
    }
    impl Navigate for ErrorNet {
        fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, _url: Url) {
            self.pending.lock().unwrap().push(FetchPayload {
                tab_id,
                generation,
                resource_id,
                result: Err(FetchError::HttpStatus(404)),
            });
        }
        fn poll_result(&self) -> Option<FetchPayload> {
            self.pending.lock().unwrap().pop()
        }
    }
    let fake: Arc<dyn Navigate> = Arc::new(ErrorNet {
        pending: Mutex::new(Vec::new()),
    });
    let mut app = App::with_net(fake);
    app.submit_url("https://example.com");
    app.step(Duration::ZERO);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("failed to load"))
    );
    assert!(app.message().contains("http status 404"));
}

#[test]
fn plain_text_is_rendered_without_html_parsing() {
    let mut app = App::new();
    app.submit_url("https://example.com/plain");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/plain").unwrap(),
            body: b"one\ntwo".to_vec(),
            content_type: Some("text/plain; charset=\"utf-8\"".to_string()),
        }),
    }));
    assert_eq!(app.tabs.active().painted.text_lines(), vec!["one", "two"]);
    assert!(app.message().contains("plain text"));
}

#[test]
fn unsupported_valid_media_types_are_not_parsed_as_html() {
    let mut app = App::new();
    app.submit_url("https://example.com/image");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/image").unwrap(),
            body: b"not really a png".to_vec(),
            content_type: Some("image/png".to_string()),
        }),
    }));
    assert!(app.tabs.active().painted.text_lines()[0].starts_with("cannot display"));
    assert_eq!(app.message(), "unsupported content type: image/png");
}

#[test]
fn embedded_author_styles_participate_in_rendering() {
    let mut app = App::new();
    app.submit_url("https://example.com/styled");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/styled").unwrap(),
            body: b"<style>p.secret { display: none }</style><p class=secret>hidden</p><div>shown</div>".to_vec(),
            content_type: Some("text/html".to_string()),
        }),
    }));
    let lines = app.tabs.active().painted.text_lines();
    assert!(lines.iter().any(|line| line == "shown"));
    assert!(!lines.iter().any(|line| line.contains("hidden")));
}

#[test]
fn valid_imports_are_scheduled_and_late_imports_warn() {
    let fake = Arc::new(FakeNet::default());
    let mut app = App::with_net(fake.clone());
    app.submit_url("https://example.com");
    let pending = fake.pending.lock().unwrap().pop().unwrap();
    assert!(
        app.deliver_fetch(FetchPayload {
            tab_id: pending.tab_id,
            generation: pending.generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: Url::parse("https://example.com/").unwrap(),
                body: br#"<!doctype html><html><head>
                <style>@import url(one.css); p { display: block }</style>
                <style>@media (width: 1px) { p { display: none } } @import url(two.css);</style>
                </head><body><p>shown</p></body></html>"#
                    .to_vec(),
                content_type: Some("text/html; charset=utf-8".to_string()),
            }),
        })
    );
    assert_eq!(fake.submitted.lock().unwrap().len(), 2);
    app.step(Duration::ZERO);
    assert_eq!(
        app.message(),
        "accepted gen 1 - https://example.com/ (0 parse errors, 1 CSS warnings, 1 stylesheets, 0 failed)"
    );
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "shown")
    );
}

#[test]
fn zero_css_warnings_preserve_the_existing_acceptance_message() {
    let mut app = App::new();
    app.submit_url("https://example.com");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/").unwrap(),
            body: b"<!doctype html><html><body><p>shown</p></body></html>".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
    }));
    assert_eq!(
        app.message(),
        "accepted gen 1 - https://example.com/ (0 parse errors)"
    );
}
