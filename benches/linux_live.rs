use std::sync::Arc;
use std::time::{Duration, Instant};

use encoding_rs::UTF_8;
use textsurfer::core::geom::Size;
use textsurfer::core::style::{Palette, RenderContext};
use textsurfer::css::{ColorScheme, DynamicState};
use textsurfer::net::FetchResponse;
use textsurfer::paint::DisplayList;
use textsurfer::pipeline::page_load::{PageLoadOptions, PendingPageLoad};
use textsurfer::pipeline::render::{
    BlockingRenderQueue, PaintBaseline, PaintUpdate, RenderKey, RenderedPage,
};
use url::Url;

const PAGE: &str = include_str!("../tests/fixtures/linux-live/page.html");
const MODULES_CSS: &str = include_str!("../tests/fixtures/linux-live/modules.css");
const SITE_CSS: &str = include_str!("../tests/fixtures/linux-live/site.css");
const SLICE_BYTES: usize = 64 * 1024;
const FRAME_BUDGET: Duration = Duration::from_micros(16_667);

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

fn apply_page(page: RenderedPage, display: &mut Arc<DisplayList>, revision: &mut RenderKey) {
    match page.paint_update {
        Some(PaintUpdate::Unchanged { base }) => assert_eq!(base, *revision),
        Some(PaintUpdate::Patch { base, patch }) => {
            assert_eq!(base, *revision);
            assert!(patch.apply(Arc::make_mut(display)));
        }
        Some(PaintUpdate::Replace(_)) => unreachable!(),
        None => *display = Arc::new(page.painted),
    }
    *revision = page.revision;
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
    let renders = BlockingRenderQueue::new();
    let result = renders
        .render(
            load.take_render_job(RenderKey {
                tab_id: 0,
                generation: 0,
                epoch: load.render_epoch(),
                hard_epoch: load.hard_epoch(),
            })
            .expect("the page is ready to render"),
        )
        .expect("the render worker returns the page");
    let wall = render_started.elapsed();
    let timings = result.timings;
    let worker = timings.cascade + timings.restyle + timings.layout + timings.paint;

    println!("linux-live — Stylo adoption gate");
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

    let page = load
        .apply_render_result(result)
        .expect("the first render publishes");
    let painted_rows = page.painted.len();
    let hovered = page
        .painted
        .links
        .first()
        .expect("the page has at least one link")
        .node;
    let mut revision = page.revision;
    let mut display = Arc::new(page.painted);
    assert!(
        load.set_dynamic_state(DynamicState {
            hover: Some(hovered),
            ..Default::default()
        })
        .is_none()
    );
    let mut hover_job = load
        .take_render_job(RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: load.render_epoch(),
            hard_epoch: load.hard_epoch(),
        })
        .expect("the hover queues a render");
    hover_job.baseline = Some(PaintBaseline {
        key: revision,
        painted: Arc::clone(&display),
    });
    let hover = renders
        .render(hover_job)
        .expect("the render worker returns the hover");
    let hover_timings = hover.timings;
    let hover_worker =
        hover_timings.cascade + hover_timings.restyle + hover_timings.layout + hover_timings.paint;
    let page = load
        .apply_render_result(hover)
        .expect("the warm-up hover publishes");
    apply_page(page, &mut display, &mut revision);

    let mut retained_samples = Vec::with_capacity(100);
    let mut worker_samples = Vec::with_capacity(100);
    let mut apply_samples = Vec::with_capacity(100);
    let mut complete_samples = Vec::with_capacity(100);
    for index in 0..100 {
        let state = if index % 2 == 0 {
            DynamicState::default()
        } else {
            DynamicState {
                hover: Some(hovered),
                ..Default::default()
            }
        };
        assert!(load.set_dynamic_state(state).is_none());
        let mut job = load
            .take_render_job(RenderKey {
                tab_id: 0,
                generation: 0,
                epoch: load.render_epoch(),
                hard_epoch: load.hard_epoch(),
            })
            .expect("the state change queues a render");
        job.baseline = Some(PaintBaseline {
            key: revision,
            painted: Arc::clone(&display),
        });
        let complete_started = Instant::now();
        let sample = renders
            .render(job)
            .expect("the render worker returns the state change");
        retained_samples.push(sample.timings.restyle);
        worker_samples.push(
            sample.timings.cascade
                + sample.timings.restyle
                + sample.timings.layout
                + sample.timings.paint,
        );
        let apply_started = Instant::now();
        let page = load
            .apply_render_result(sample)
            .expect("the state change publishes");
        apply_page(page, &mut display, &mut revision);
        apply_samples.push(apply_started.elapsed());
        complete_samples.push(complete_started.elapsed());
    }
    retained_samples.sort_unstable();
    worker_samples.sort_unstable();
    apply_samples.sort_unstable();
    complete_samples.sort_unstable();
    let retained_p95 = retained_samples[94];
    let worker_p95 = worker_samples[94];
    let apply_p95 = apply_samples[94];
    let complete_p95 = complete_samples[94];

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
    println!(
        "  retained p95      : {:>7.1} ms",
        retained_p95.as_secs_f64() * 1000.0
    );
    println!(
        "  retained worker p95: {:>6.1} ms",
        worker_p95.as_secs_f64() * 1000.0
    );
    println!(
        "  owner apply p95   : {:>7.1} ms",
        apply_p95.as_secs_f64() * 1000.0
    );
    println!(
        "  retained total p95: {:>7.1} ms",
        complete_p95.as_secs_f64() * 1000.0
    );
    println!("  painted rows      : {painted_rows}");
    assert!(retained_p95 < FRAME_BUDGET);
    assert!(complete_p95 < FRAME_BUDGET);
}
