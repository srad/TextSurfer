use std::collections::{BTreeMap, VecDeque};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use textsurfer::app::App;
use textsurfer::app::net::{Navigate, PoolNet};
use textsurfer::core::event::{InputBatch, InputEvent, ResizePhase};
use textsurfer::core::geom::Size;
use textsurfer::core::style::RenderMetrics;
use textsurfer::net::{
    Fetch, FetchPayload, FetchPoll, FetchPool, FetchResponse, ResourceId, Submitted, UreqFetch,
};
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

const PAGE: &str = include_str!("../tests/fixtures/linux-live/page.html");
const MODULES_CSS: &str = include_str!("../tests/fixtures/linux-live/modules.css");
const SITE_CSS: &str = include_str!("../tests/fixtures/linux-live/site.css");
const IMAGE: &[u8] = &[
    71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 33, 249, 4, 1, 0, 0, 0,
    0, 44, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
];
const INITIAL_SIZE: Size = Size {
    cols: 160,
    rows: 50,
};
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "https://en.wikipedia.org/wiki/Linux")]
    url: String,
    #[arg(long, default_value_t = 5)]
    runs: usize,
    #[arg(long, default_value = "140x45", value_parser = parse_size)]
    resize: Size,
    #[arg(long)]
    fixture: bool,
    #[arg(long = "bench", hide = true)]
    _bench: bool,
}

fn parse_size(value: &str) -> Result<Size, String> {
    let (cols, rows) = value
        .split_once('x')
        .ok_or_else(|| "size must be COLSxROWS".to_string())?;
    let cols = cols
        .parse::<u16>()
        .map_err(|_| "columns must fit in u16".to_string())?;
    let rows = rows
        .parse::<u16>()
        .map_err(|_| "rows must fit in u16".to_string())?;
    Ok(Size { cols, rows })
}

#[derive(Clone)]
struct PerfEvent {
    at: Instant,
    message: String,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct EventStore {
    events: Arc<Mutex<Vec<PerfEvent>>>,
}

impl EventStore {
    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    fn snapshot(&self) -> Vec<PerfEvent> {
        self.events.lock().unwrap().clone()
    }
}

struct PerfLayer {
    store: EventStore,
}

impl<S> Layer<S> for PerfLayer
where
    S: Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "textsurfer::perf" {
            return;
        }
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        let message = visitor.fields.remove("message").unwrap_or_default();
        self.store.events.lock().unwrap().push(PerfEvent {
            at: Instant::now(),
            message,
            fields: visitor.fields,
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl FieldVisitor {
    fn insert(&mut self, field: &Field, value: impl ToString) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl Visit for FieldVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        let value = format!("{value:?}");
        self.insert(field, value.trim_matches('"'));
    }
}

struct FixtureNet {
    pending: Mutex<VecDeque<(Instant, FetchPayload)>>,
}

impl FixtureNet {
    fn new() -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
        }
    }

    fn response(resource_id: ResourceId, url: Url) -> FetchResponse {
        let query = url.query().unwrap_or_default();
        let (status, body, content_type) = if resource_id == ResourceId::DOCUMENT {
            (200, PAGE.as_bytes().to_vec(), Some("text/html".to_string()))
        } else if url.path().contains("load.php") && query.contains("modules=site.styles") {
            (
                200,
                SITE_CSS.as_bytes().to_vec(),
                Some("text/css".to_string()),
            )
        } else if url.path().contains("load.php") {
            (
                200,
                MODULES_CSS.as_bytes().to_vec(),
                Some("text/css".to_string()),
            )
        } else {
            (200, IMAGE.to_vec(), Some("image/gif".to_string()))
        };
        FetchResponse {
            final_url: url,
            status,
            body,
            content_type,
        }
    }
}

impl Navigate for FixtureNet {
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) -> Submitted {
        let delay = if resource_id == ResourceId::DOCUMENT {
            Duration::ZERO
        } else if url.path().contains("load.php") {
            Duration::from_millis(300)
        } else {
            Duration::from_millis(600)
        };
        self.pending.lock().unwrap().push_back((
            Instant::now() + delay,
            FetchPayload {
                tab_id,
                generation,
                resource_id,
                result: Ok(Self::response(resource_id, url)),
            },
        ));
        Submitted::Queued
    }

    fn poll_result(&self) -> FetchPoll {
        let mut pending = self.pending.lock().unwrap();
        if pending
            .front()
            .is_some_and(|(ready, _)| *ready <= Instant::now())
        {
            return FetchPoll::Ready(pending.pop_front().unwrap().1);
        }
        FetchPoll::Empty
    }
}

struct DriveResult {
    wall: Duration,
    max_step: Duration,
}

fn drive_until_idle(
    app: &mut App,
    started: Instant,
    timeout: Duration,
) -> Result<DriveResult, String> {
    let mut max_step = Duration::ZERO;
    loop {
        let now = started.elapsed();
        let step = Instant::now();
        app.advance(&InputBatch::new(), now);
        max_step = max_step.max(step.elapsed());
        if app.next_wake().is_none() {
            return Ok(DriveResult {
                wall: started.elapsed(),
                max_step,
            });
        }
        if started.elapsed() >= timeout {
            return Err(format!("timed out after {:.1} s", timeout.as_secs_f64()));
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn resize(app: &mut App, started: Instant, target: Size) -> Result<DriveResult, String> {
    for size in [
        Size {
            cols: INITIAL_SIZE.cols.saturating_sub(4),
            rows: INITIAL_SIZE.rows.saturating_sub(2),
        },
        Size {
            cols: target.cols.saturating_add(3),
            rows: target.rows.saturating_add(1),
        },
        target,
    ] {
        let event = InputEvent::Resize {
            size,
            phase: ResizePhase::Preview,
        };
        app.advance(&InputBatch::from(event), started.elapsed());
        thread::sleep(Duration::from_millis(10));
    }
    drive_until_idle(app, started, RUN_TIMEOUT)
}

fn published(events: &[PerfEvent]) -> usize {
    events
        .iter()
        .filter(|event| event.message == "render result published")
        .count()
}

fn first_publish(events: &[PerfEvent], started: Instant) -> Option<Duration> {
    events
        .iter()
        .find(|event| event.message == "render result published")
        .map(|event| event.at.saturating_duration_since(started))
}

fn timing_values(events: &[PerfEvent], name: &str) -> Vec<u64> {
    events
        .iter()
        .filter(|event| event.message == "render result received")
        .filter_map(|event| event.fields.get(name)?.parse().ok())
        .collect()
}

fn run(args: &Args, store: &EventStore, index: usize) -> Result<(), String> {
    let net: Arc<dyn Navigate> = if args.fixture {
        Arc::new(FixtureNet::new())
    } else {
        let fetch: Arc<dyn Fetch> = Arc::new(UreqFetch::new());
        Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))))
    };
    let mut app = App::with_net_and_metrics(net, RenderMetrics::VGA);
    app.on_resize(INITIAL_SIZE);
    store.clear();
    let started = Instant::now();
    app.submit_url(&args.url);
    let initial = drive_until_idle(&mut app, started, RUN_TIMEOUT)?;
    let initial_events = store.snapshot();
    if published(&initial_events) == 0 {
        return Err(format!("no page published; status={}", app.message()));
    }
    let resize_started = started.elapsed();
    let resize_event_index = initial_events.len();
    let resized = resize(&mut app, started, args.resize)?;
    let all_events = store.snapshot();
    let resize_events = &all_events[resize_event_index..];
    if published(resize_events) == 0 {
        return Err(format!(
            "resize published no page; status={}",
            app.message()
        ));
    }
    app.shutdown_net();
    let initial_cascade = timing_values(&initial_events, "cascade_ms");
    let initial_layout = timing_values(&initial_events, "layout_ms");
    let initial_paint = timing_values(&initial_events, "paint_ms");
    let resize_cascade = timing_values(resize_events, "cascade_ms");
    let resize_layout = timing_values(resize_events, "layout_ms");
    let resize_paint = timing_values(resize_events, "paint_ms");
    println!("run={index}");
    println!(
        "first_publish_ms={:.1}",
        first_publish(&initial_events, started)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0
    );
    println!("settled_ms={:.1}", initial.wall.as_secs_f64() * 1000.0);
    println!(
        "max_owner_step_ms={:.1}",
        initial.max_step.max(resized.max_step).as_secs_f64() * 1000.0
    );
    println!(
        "resize_settled_ms={:.1}",
        resized.wall.saturating_sub(resize_started).as_secs_f64() * 1000.0
    );
    println!("initial_publish_count={}", published(&initial_events));
    println!("initial_cascade_ms={initial_cascade:?}");
    println!("initial_layout_ms={initial_layout:?}");
    println!("initial_paint_ms={initial_paint:?}");
    println!("resize_publish_count={}", published(resize_events));
    println!("resize_cascade_ms={resize_cascade:?}");
    println!("resize_layout_ms={resize_layout:?}");
    println!("resize_paint_ms={resize_paint:?}");
    Ok(())
}

fn main() {
    let args = Args::parse();
    let store = EventStore::default();
    tracing_subscriber::registry()
        .with(PerfLayer {
            store: store.clone(),
        })
        .init();
    for index in 1..=args.runs {
        if let Err(error) = run(&args, &store, index) {
            eprintln!("app_render failed: {error}");
            std::process::exit(1);
        }
    }
}
