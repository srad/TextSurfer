use std::time::{Duration, Instant};

use encoding_rs::UTF_8;
use textsurfer::core::geom::Size;
use textsurfer::core::style::{Palette, RenderContext};
use textsurfer::css::ColorScheme;
use textsurfer::pipeline::page_load::{PageLoadOptions, PendingPageLoad};
use textsurfer::pipeline::render::RenderKey;
use url::Url;

const SOURCE: &str = include_str!("../tests/fixtures/linux-revision-1371530035.html");
const SLICE_BYTES: usize = 64 * 1024;

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
        SOURCE.to_string(),
        Url::parse("https://en.wikipedia.org/wiki/Linux").unwrap(),
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
    let snapshot_started = Instant::now();
    assert!(load.render_if_ready(Duration::from_secs(5)).is_none());
    let result = load
        .take_render_job(RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: load.render_epoch(),
            hard_epoch: load.hard_epoch(),
        })
        .unwrap()
        .execute();
    let snapshot_and_render = snapshot_started.elapsed();
    let timings = result.timings;
    let render = timings.cascade + timings.layout + timings.paint;
    let snapshot = snapshot_and_render.saturating_sub(render);
    let page = load.apply_render_result(result).unwrap();
    println!(
        "linux revision 1371530035: bytes={} slices={} parse+discovery_ms={} max_parse_slice_ms={} snapshot_ms={} render_ms={} cascade_ms={} layout_ms={} paint_ms={} rows={} images={}",
        SOURCE.len(),
        slices,
        parse_and_discovery.as_millis(),
        max_owner_slice.as_millis(),
        snapshot.as_millis(),
        render.as_millis(),
        timings.cascade.as_millis(),
        timings.layout.as_millis(),
        timings.paint.as_millis(),
        page.painted.len(),
        page.painted.images.len(),
    );
}
