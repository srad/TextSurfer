//! The M7 perf baseline: the full live Wikipedia page, with both of its stylesheets, through the
//! custom cascade.
//!
//! `linux_page.rs` measures the Parsoid article body alone, which links no external CSS and so
//! exercises a much smaller cascade. The external stylesheets are what make the custom cascade
//! expensive, and therefore what M7 exists to attack — so this is the number the Stylo migration is
//! judged against.
//!
//! Nothing is fetched: the stylesheet bodies are handed to `PageLoad::deliver` against the resource
//! ids discovery reports, the way the `FakeFetch` harnesses do it.

use std::time::{Duration, Instant};

use encoding_rs::UTF_8;
use textsurfer::core::geom::Size;
use textsurfer::core::style::{Palette, RenderContext};
use textsurfer::css::{ColorScheme, DynamicState};
use textsurfer::net::FetchResponse;
use textsurfer::pipeline::page_load::{PageLoadOptions, PendingPageLoad};
use textsurfer::pipeline::render::RenderKey;
use url::Url;

const PAGE: &str = include_str!("../tests/fixtures/linux-live/page.html");
const MODULES_CSS: &str = include_str!("../tests/fixtures/linux-live/modules.css");
const SITE_CSS: &str = include_str!("../tests/fixtures/linux-live/site.css");
const SLICE_BYTES: usize = 64 * 1024;

/// Answer a stylesheet request from the fixtures rather than the network.
///
/// The page links the module bundle first and `site.styles` second; match on the query rather than
/// the position so a re-capture that reorders them still resolves correctly.
fn stylesheet_for(url: &Url) -> Option<&'static str> {
    let query = url.query().unwrap_or_default();
    if !url.path().contains("load.php") {
        return None;
    }
    if query.contains("modules=site.styles") {
        Some(SITE_CSS)
    } else {
        Some(MODULES_CSS)
    }
}

fn main() {
    let options = PageLoadOptions {
        render: RenderContext::terminal(Size {
            cols: 120,
            rows: 40,
        }),
        palette: Palette::default(),
        scripting: false,
        color_scheme: ColorScheme::Dark,
        started: Duration::ZERO,
    };
    let mut pending = PendingPageLoad::new(
        PAGE.to_string(),
        Url::parse("https://en.wikipedia.org/wiki/Linux").expect("the fixture base url parses"),
        UTF_8,
        options,
    );

    let total_started = Instant::now();
    let mut max_owner_slice = Duration::ZERO;
    let mut slices = 0;
    let mut load = loop {
        let started = Instant::now();
        let completed = pending.step(SLICE_BYTES);
        max_owner_slice = max_owner_slice.max(started.elapsed());
        slices += 1;
        if let Some(load) = completed {
            break load;
        }
    };
    let parse_and_discovery = total_started.elapsed();

    let mut delivered = 0;
    let mut refused = Vec::new();
    let mut commands = load.take_commands();
    while !commands.is_empty() {
        for command in commands {
            match stylesheet_for(&command.url) {
                Some(css) => {
                    load.deliver(
                        command.resource_id,
                        Ok(FetchResponse {
                            final_url: command.url.clone(),
                            status: 200,
                            body: css.as_bytes().to_vec(),
                            content_type: Some("text/css".to_string()),
                        }),
                    );
                    delivered += 1;
                }
                None => {
                    refused.push(command.url.to_string());
                    load.deliver(
                        command.resource_id,
                        Ok(FetchResponse {
                            final_url: command.url.clone(),
                            status: 404,
                            body: Vec::new(),
                            content_type: None,
                        }),
                    );
                }
            }
        }
        // `@import` inside a delivered sheet queues more work.
        commands = load.take_commands();
    }
    // Settles the blocking window the way the owner sequence would.
    let _ = load.render_if_ready(Duration::from_secs(5));

    let render_started = Instant::now();
    let result = load
        .take_render_job(RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: load.render_epoch(),
            hard_epoch: load.hard_epoch(),
        })
        .expect("the page is ready to render")
        .execute();
    let wall = render_started.elapsed();
    let timings = result.timings;
    let worker = timings.cascade + timings.restyle + timings.layout + timings.paint;

    println!("linux-live — custom cascade baseline (M7 gate)");
    println!(
        "  parse + discovery : {:>7.1} ms across {slices} slices",
        parse_and_discovery.as_secs_f64() * 1000.0
    );
    println!(
        "  max owner slice   : {:>7.1} ms",
        max_owner_slice.as_secs_f64() * 1000.0
    );
    println!("  stylesheets       : {delivered} delivered from fixtures");
    if !refused.is_empty() {
        println!("  UNRESOLVED        : {refused:?}");
    }
    println!(
        "  cascade           : {:>7.1} ms",
        timings.cascade.as_secs_f64() * 1000.0
    );
    println!(
        "  restyle           : {:>7.1} ms",
        timings.restyle.as_secs_f64() * 1000.0
    );
    println!(
        "  layout            : {:>7.1} ms",
        timings.layout.as_secs_f64() * 1000.0
    );
    println!(
        "  paint             : {:>7.1} ms",
        timings.paint.as_secs_f64() * 1000.0
    );
    println!(
        "  worker render     : {:>7.1} ms",
        worker.as_secs_f64() * 1000.0
    );
    println!(
        "  wall (incl. snapshot): {:>4.1} ms",
        wall.as_secs_f64() * 1000.0
    );

    // The hover path is the one M7 actually attacks: layout is retained, so the cascade is the
    // dominant cost and the whole frame budget rides on it.
    let page = load
        .apply_render_result(result)
        .expect("the first render publishes");
    let hovered = page
        .painted
        .links
        .first()
        .expect("the page has at least one link")
        .node;
    assert!(
        load.set_dynamic_state(DynamicState {
            hover: Some(hovered),
            ..Default::default()
        })
        .is_none()
    );
    let hover = load
        .take_render_job(RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: load.render_epoch(),
            hard_epoch: load.hard_epoch(),
        })
        .expect("the hover queues a render")
        .execute();
    let hover_timings = hover.timings;
    let hover_worker =
        hover_timings.cascade + hover_timings.restyle + hover_timings.layout + hover_timings.paint;

    println!();
    println!(
        "  hover cascade     : {:>7.1} ms",
        hover_timings.cascade.as_secs_f64() * 1000.0
    );
    println!(
        "  hover restyle     : {:>7.1} ms",
        hover_timings.restyle.as_secs_f64() * 1000.0
    );
    println!(
        "  hover layout      : {:>7.1} ms",
        hover_timings.layout.as_secs_f64() * 1000.0
    );
    println!(
        "  hover paint       : {:>7.1} ms",
        hover_timings.paint.as_secs_f64() * 1000.0
    );
    println!(
        "  hover worker      : {:>7.1} ms",
        hover_worker.as_secs_f64() * 1000.0
    );
    println!("  painted rows      : {}", page.painted.len());
}
