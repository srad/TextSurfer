use super::*;

struct RefuseOnceRenderQueue {
    refused: std::sync::atomic::AtomicBool,
    blocking: crate::pipeline::render::BlockingRenderQueue,
}

impl crate::pipeline::render::RenderQueue for RefuseOnceRenderQueue {
    fn submit(
        &self,
        job: crate::pipeline::render::RenderJob,
    ) -> crate::pipeline::render::RenderSubmitted {
        if !self.refused.swap(true, std::sync::atomic::Ordering::AcqRel) {
            crate::pipeline::render::RenderSubmitted::Refused(job)
        } else {
            self.blocking.submit(job)
        }
    }

    fn poll(&self) -> crate::pipeline::render::RenderPoll {
        self.blocking.poll()
    }
}

#[test]
fn a_refused_render_job_is_retried_without_losing_its_invalidation() {
    let net = Arc::new(FakeNet::default());
    let renders = Arc::new(RefuseOnceRenderQueue {
        refused: std::sync::atomic::AtomicBool::new(false),
        blocking: crate::pipeline::render::BlockingRenderQueue::default(),
    });
    let mut app = App::with_net_and_render_queue(net, renders.clone());
    app.submit_url("https://example.com/");
    app.step(Duration::ZERO);
    assert!(renders.refused.load(std::sync::atomic::Ordering::Acquire));
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("hi there"))
    );
}

#[test]
fn a_large_document_is_parsed_across_owner_ticks() {
    let net = Arc::new(FakeNet::default());
    let mut app = App::with_net(net.clone());
    app.submit_url("https://example.com/large");
    let document = net.pending.lock().unwrap().pop().unwrap();
    let html = format!(
        "<!doctype html><!--{}--><p id=complete>large complete</p>",
        "x".repeat(128 * 1024)
    );
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/large").unwrap(),
            status: 200,
            body: html.into_bytes(),
            content_type: Some("text/html".to_string()),
        }),
        ..document
    }));
    assert!(app.tabs.active().pending_load.is_some());
    assert!(app.tabs.active().document.is_none());
    app.step(Duration::from_millis(1));
    assert!(app.tabs.active().pending_load.is_some());
    app.step(Duration::from_millis(2));
    app.step(Duration::from_millis(3));
    assert!(app.tabs.active().pending_load.is_none());
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("large complete"))
    );
}

#[test]
fn image_fetches_route_through_the_injected_decoder_by_tab_and_generation() {
    let net = Arc::new(FakeNet::default());
    let images = Arc::new(FakeImages::default());
    let mut app = App::with_net_metrics_and_images(
        net.clone(),
        crate::core::style::RenderMetrics::TERMINAL,
        images.clone(),
    );
    app.submit_url("https://example.com/page");
    let document = net.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/page").unwrap(),
            status: 200,
            body: b"<img id=hero src=hero.png>".to_vec(),
            content_type: Some("text/html".to_string()),
        }),
        ..document
    }));
    let image_fetch = net.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/hero.png").unwrap(),
            status: 200,
            body: vec![1, 2, 3],
            content_type: Some("text/plain".to_string()),
        }),
        ..image_fetch
    }));
    let job = images.submitted.lock().unwrap().pop().unwrap();
    assert_eq!(job.tab_id, app.tabs.active().id);
    assert_eq!(job.generation, app.tabs.active().generation);
    images.pending.lock().unwrap().push(ImageDecodePayload {
        tab_id: job.tab_id,
        generation: job.generation,
        asset_id: job.request.asset_id,
        revision: job.request.revision,
        result: Ok(crate::core::image::DecodedImage {
            asset_id: job.request.asset_id,
            revision: job.request.revision,
            width: 1,
            height: 1,
            rgba: Arc::from([7, 8, 9, 255]),
        }),
    });
    app.step(Duration::ZERO);
    let tab = app.tabs.active();
    let node = tab
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("hero")
        .expect("image node");
    let load = tab.load.as_ref().unwrap();
    assert_eq!(
        load.decoded_image(node).unwrap().rgba.as_ref(),
        &[7, 8, 9, 255]
    );
}

#[test]
fn image_status_separates_rate_limits_from_unknown_formats() {
    let net = Arc::new(FakeNet::default());
    let images = Arc::new(FakeImages::default());
    let mut app = App::with_net_metrics_and_images(
        net.clone(),
        crate::core::style::RenderMetrics::TERMINAL,
        images.clone(),
    );
    app.submit_url("https://example.com/page");
    let document = net.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/page").unwrap(),
            status: 200,
            body: b"<img src=limited.png><img src=vector.svg>".to_vec(),
            content_type: Some("text/html".to_string()),
        }),
        ..document
    }));
    let pending = std::mem::take(&mut *net.pending.lock().unwrap());
    for payload in pending {
        let url = payload.result.as_ref().unwrap().final_url.clone();
        if url.path().ends_with("limited.png") {
            assert!(app.deliver_fetch(FetchPayload {
                result: Ok(FetchResponse {
                    final_url: url,
                    status: 429,
                    body: b"slow down".to_vec(),
                    content_type: Some("text/plain".to_string()),
                }),
                ..payload
            }));
        } else {
            assert!(app.deliver_fetch(FetchPayload {
                result: Ok(FetchResponse {
                    final_url: url,
                    status: 200,
                    body: b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
                    content_type: Some("image/svg+xml".to_string()),
                }),
                ..payload
            }));
        }
    }
    let job = images.submitted.lock().unwrap().pop().unwrap();
    assert!(app.deliver_image_decode(ImageDecodePayload {
        tab_id: job.tab_id,
        generation: job.generation,
        asset_id: job.request.asset_id,
        revision: job.request.revision,
        result: Err(crate::core::image::ImageDecodeError::UnknownFormat),
    }));
    assert_eq!(
        app.message(),
        "loaded https://example.com/page (1 image rate-limited, 1 image format unsupported or unrecognized)"
    );
}

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
            status: 200,
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
            status: 200,
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
            status: 200,
            body: vec![],
            content_type: Some("text/html".to_string()),
        }),
    }));
    assert_eq!(app.message(), message);
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/").unwrap(),
            status: 200,
            body: vec![],
            content_type: Some("text/html".to_string()),
        }),
    }));
    assert_eq!(app.message(), "loaded https://example.com/");
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
            status: 200,
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
    assert_eq!(app.message(), "loaded https://a.example/final");
}

#[test]
fn closing_the_front_tab_renders_the_one_that_takes_its_place() {
    // Closing changes which tab is in front, so like every other tab switch it owes the
    // newcomer a render; otherwise its finished load waits for an unrelated keystroke.
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
            status: 200,
            body: b"<p>background complete</p>".to_vec(),
            content_type: Some("text/html; charset=\"utf-8\"".to_string()),
        }),
    }));
    app.close_tab_at(app.tabs.active_index());
    assert_eq!(app.tab_count(), 1);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("background complete"))
    );
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
    assert_eq!(app.message(), "loaded https://example.com/");
}

#[test]
fn fetch_errors_land_in_the_tab_content() {
    struct ErrorNet {
        pending: Mutex<Vec<FetchPayload>>,
    }
    impl Navigate for ErrorNet {
        fn submit(
            &self,
            tab_id: u64,
            generation: u64,
            resource_id: ResourceId,
            _url: Url,
        ) -> Submitted {
            self.pending.lock().unwrap().push(FetchPayload {
                tab_id,
                generation,
                resource_id,
                result: Err(FetchError::HttpStatus(404)),
            });
            Submitted::Queued
        }
        fn poll_result(&self) -> FetchPoll {
            self.pending
                .lock()
                .unwrap()
                .pop()
                .map_or(FetchPoll::Empty, FetchPoll::Ready)
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
            status: 200,
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
            status: 200,
            body: b"not really a png".to_vec(),
            content_type: Some("image/png".to_string()),
        }),
    }));
    assert!(app.tabs.active().painted.text_lines()[0].starts_with("cannot display"));
    assert_eq!(app.message(), "unsupported content type: image/png");
}

#[test]
fn a_missing_media_type_is_sniffed_before_document_rendering() {
    let mut app = App::new();
    app.submit_url("https://example.com/untyped");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/untyped").unwrap(),
            status: 200,
            body: b"\n<h1>sniffed html</h1>".to_vec(),
            content_type: None,
        }),
    }));
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("sniffed html"))
    );
}

#[test]
fn a_missing_media_type_never_routes_binary_bytes_to_html() {
    let mut app = App::new();
    app.submit_url("https://example.com/binary");
    let generation = app.tabs.active().generation;
    let tab_id = app.tabs.active().id;
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id: ResourceId::DOCUMENT,
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/binary").unwrap(),
            status: 200,
            body: b"binary\0body".to_vec(),
            content_type: None,
        }),
    }));
    assert!(app.tabs.active().painted.text_lines()[0].starts_with("cannot display"));
    assert_eq!(
        app.message(),
        "unsupported content type: application/octet-stream"
    );
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
            status: 200,
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
                status: 200,
                body: br#"<!doctype html><html><head>
                <style>@import url(one.css); p { display: block }</style>
                <style>@media (width: 1ch) { p { display: none } } @import url(two.css);</style>
                </head><body><p>shown</p></body></html>"#
                    .to_vec(),
                content_type: Some("text/html; charset=utf-8".to_string()),
            }),
        })
    );
    assert_eq!(fake.submitted.lock().unwrap().len(), 2);
    app.step(Duration::ZERO);
    assert_eq!(app.message(), "loaded https://example.com/");
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
fn failed_stylesheets_remain_visible_in_the_load_status() {
    let fake = Arc::new(FakeNet::serving(
        "<!doctype html><link rel=stylesheet href=missing.css><p>shown</p>",
    ));
    let mut app = App::with_net(fake.clone());
    app.submit_url("https://example.com");
    let document = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(document));
    let stylesheet = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Err(FetchError::Network("missing".to_string())),
        ..stylesheet
    }));
    assert_eq!(
        app.message(),
        "loaded https://example.com/ (1 stylesheet failed)"
    );
}

#[test]
fn successful_html_reports_a_reader_facing_load_message() {
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
            status: 200,
            body: b"<!doctype html><html><body><p>shown</p></body></html>".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
    }));
    assert_eq!(app.message(), "loaded https://example.com/");
}

#[test]
fn recoverable_html_parse_errors_are_not_reported_as_load_failures() {
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
            status: 200,
            body: b"<!doctype html><p>shown</p".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
    }));
    assert_eq!(app.message(), "loaded https://example.com/");
}

#[test]
fn duckduckgo_noscript_refresh_replaces_the_wrapper_and_loads_wikipedia() {
    let fake = Arc::new(FakeNet::default());
    let mut app = App::with_net(fake.clone());
    let wrapper = "https://duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FCentral_processing_unit&rut=token";
    app.submit_url(wrapper);
    let request = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse(wrapper).unwrap(),
            status: 200,
            body: b"<noscript><meta http-equiv='refresh' content='0;URL=https://en.wikipedia.org/wiki/Central_processing_unit'></noscript>".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
        ..request
    }));
    assert_eq!(
        app.active_url(),
        "https://en.wikipedia.org/wiki/Central_processing_unit"
    );
    assert_eq!(
        app.tabs.active().history,
        ["https://en.wikipedia.org/wiki/Central_processing_unit"]
    );
    let wikipedia = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(wikipedia));
    assert_eq!(
        app.message(),
        "loaded https://en.wikipedia.org/wiki/Central_processing_unit"
    );
}

#[test]
fn a_background_tabs_declarative_refresh_stays_in_that_tab() {
    let fake = Arc::new(FakeNet::default());
    let mut app = App::with_net(fake.clone());
    app.submit_url("https://example.com/wrapper");
    app.new_tab();
    let wrapper = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        result: Ok(FetchResponse {
            final_url: Url::parse("https://example.com/wrapper").unwrap(),
            status: 200,
            body: b"<meta http-equiv=refresh content='0;url=/destination'>".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        }),
        ..wrapper
    }));
    assert!(app.active_url().is_empty());
    assert_eq!(app.tabs.tabs()[0].url, "https://example.com/destination");
    let destination = fake.pending.lock().unwrap().pop().unwrap();
    assert!(app.deliver_fetch(destination));
    assert_eq!(
        app.tabs.tabs()[0].message,
        "loading https://example.com/destination (0 stylesheets)"
    );
    assert!(app.active_url().is_empty());
}

#[test]
fn automatic_declarative_refresh_chains_are_bounded() {
    let fake = Arc::new(FakeNet::serving(
        "<meta http-equiv=refresh content='0;url=/loop'>",
    ));
    let mut app = App::with_net(fake);
    app.submit_url("https://example.com/loop");
    app.step(Duration::ZERO);
    app.step(Duration::from_millis(1));
    assert_eq!(app.message(), "automatic redirect limit reached");
    assert!(!app.tabs.active().document_pending);
}

#[test]
fn one_step_cannot_be_starved_by_an_unending_result_source() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct UnendingNet {
        polls: AtomicUsize,
    }

    impl Navigate for UnendingNet {
        fn submit(
            &self,
            _tab_id: u64,
            _generation: u64,
            _resource_id: ResourceId,
            _url: Url,
        ) -> Submitted {
            Submitted::Queued
        }

        fn poll_result(&self) -> FetchPoll {
            let poll = self.polls.fetch_add(1, Ordering::Relaxed) + 1;
            assert!(poll <= 8, "one app step exceeded its result budget");
            FetchPoll::Ready(FetchPayload {
                tab_id: u64::MAX,
                generation: u64::MAX,
                resource_id: ResourceId::DOCUMENT,
                result: Err(FetchError::Network("stale".to_string())),
            })
        }
    }

    let net = Arc::new(UnendingNet {
        polls: AtomicUsize::new(0),
    });
    let mut app = App::with_net(net.clone());
    app.step(Duration::ZERO);
    assert_eq!(net.polls.load(Ordering::Relaxed), 8);
}

#[test]
fn a_lost_fetch_pool_tells_every_waiting_tab_instead_of_leaving_it_on_loading() {
    struct DeadNet;

    impl Navigate for DeadNet {
        fn submit(
            &self,
            _tab_id: u64,
            _generation: u64,
            _resource_id: ResourceId,
            _url: Url,
        ) -> Submitted {
            Submitted::Queued
        }

        fn poll_result(&self) -> FetchPoll {
            FetchPoll::Disconnected
        }
    }

    let mut app = App::with_net(Arc::new(DeadNet));
    app.submit_url("https://example.com");
    assert!(
        app.tabs.active().document_pending,
        "the load starts out pending"
    );
    app.step(Duration::ZERO);
    assert!(
        !app.tabs.active().document_pending,
        "a pending load that can never complete must not stay pending"
    );
    assert!(
        app.message().contains("network stopped responding"),
        "the user is told the pool is gone, got {:?}",
        app.message()
    );
}

#[test]
fn a_pool_that_refuses_the_job_does_not_leave_the_tab_loading_forever() {
    struct ClosedNet;

    impl Navigate for ClosedNet {
        fn submit(
            &self,
            _tab_id: u64,
            _generation: u64,
            _resource_id: ResourceId,
            _url: Url,
        ) -> Submitted {
            Submitted::Closed
        }

        fn poll_result(&self) -> FetchPoll {
            FetchPoll::Empty
        }
    }

    let mut app = App::with_net(Arc::new(ClosedNet));
    app.submit_url("https://example.com");
    assert!(
        !app.tabs.active().document_pending,
        "a job nothing accepted is not pending"
    );
    assert!(
        app.message().contains("the network is not running"),
        "got {:?}",
        app.message()
    );
}
