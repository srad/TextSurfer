use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use url::Url;

use crate::core::geom::Size;
use crate::core::style::{Palette, RenderContext};
use crate::core::url::url_fix;
use crate::css::ColorScheme;
use crate::net::{
    Fetch, FetchPayload, FetchPoll, FetchPool, FetchRequest, FetchResponse, ResourceId, Submitted,
    charset_from_content_type, decode, decode_text,
};
use crate::pipeline::page_load::{
    MAX_DECLARATIVE_REFRESHES, PageLoad, PageLoadOptions, STYLESHEET_DEADLINE,
};
use crate::pipeline::render::{ResponseKind, response_kind};
use crate::script::JsEngineFactory;

pub fn dump_lines(
    fetch: Arc<dyn Fetch>,
    url: &str,
    viewport: Size,
    palette: Palette,
) -> io::Result<Vec<String>> {
    dump_lines_with_scripts(fetch, url, viewport, palette, None)
}

pub fn dump_lines_with_scripts(
    fetch: Arc<dyn Fetch>,
    url: &str,
    viewport: Size,
    palette: Palette,
    script_factory: Option<Arc<dyn JsEngineFactory>>,
) -> io::Result<Vec<String>> {
    let fixed = url_fix(url);
    let mut document_url = Url::parse(&fixed)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let pool = FetchPool::spawn(fetch, 4);
    let viewport = Size {
        cols: viewport.cols.max(1),
        rows: viewport.rows.max(1),
    };
    let mut redirects = 0;
    let mut generation = 0;
    loop {
        let response = fetch_document(&pool, generation, document_url)?;
        let charset = response
            .content_type
            .as_deref()
            .and_then(charset_from_content_type);
        match response_kind(response.content_type.as_deref(), &response.body) {
            ResponseKind::Html => {
                let decoded = decode(&response.body, charset.as_deref());
                let options = PageLoadOptions {
                    render: RenderContext::terminal(viewport),
                    palette,
                    scripting: script_factory.is_some(),
                    color_scheme: ColorScheme::Dark,
                    started: Duration::ZERO,
                };
                let mut load = match script_factory.clone() {
                    Some(factory) => PageLoad::new_with_scripts(
                        &decoded.text,
                        response.final_url,
                        decoded.encoding,
                        options,
                        factory,
                    ),
                    None => {
                        PageLoad::new(&decoded.text, response.final_url, decoded.encoding, options)
                    }
                };
                if let Some(url) = load.immediate_refresh().cloned() {
                    if redirects >= MAX_DECLARATIVE_REFRESHES {
                        return Err(io::Error::other("automatic redirect limit reached"));
                    }
                    redirects += 1;
                    generation += 1;
                    document_url = url;
                    continue;
                }
                let started = Instant::now();
                loop {
                    if load.scripts_runnable_at(Duration::ZERO) {
                        let _ = load.advance_scripts(Duration::ZERO);
                    }
                    for command in load.take_commands() {
                        pool.submit(
                            0,
                            generation,
                            command.resource_id,
                            FetchRequest::get(command.url),
                        );
                    }
                    if load.take_cancel_requested() {
                        pool.cancel(0, generation);
                    }
                    if load.applicable_is_settled() || started.elapsed() >= STYLESHEET_DEADLINE {
                        let detach_pool = !load.is_settled();
                        pool.cancel(0, generation);
                        if detach_pool {
                            pool.shutdown_without_waiting();
                        }
                        break;
                    }
                    match pool.try_recv() {
                        FetchPoll::Ready(FetchPayload {
                            resource_id,
                            result,
                            ..
                        }) => {
                            let _ = load.deliver(resource_id, result);
                        }
                        FetchPoll::Disconnected => break,
                        FetchPoll::Empty => std::thread::sleep(Duration::from_millis(1)),
                    }
                }
                return Ok(load.force_render().painted.text_lines());
            }
            ResponseKind::PlainText => {
                return Ok(decode_text(&response.body, charset.as_deref())
                    .text
                    .lines()
                    .map(str::to_string)
                    .collect());
            }
            ResponseKind::Unsupported(kind) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("unsupported content type: {kind}"),
                ));
            }
        }
    }
}

fn fetch_document(pool: &FetchPool, generation: u64, url: Url) -> io::Result<FetchResponse> {
    let submitted = pool.submit(0, generation, ResourceId::DOCUMENT, FetchRequest::get(url));
    if submitted != Submitted::Queued {
        return Err(io::Error::other("the network is not running"));
    }
    loop {
        match pool.try_recv() {
            FetchPoll::Ready(payload) => {
                return payload
                    .result
                    .map_err(|error| io::Error::other(error.to_string()));
            }
            FetchPoll::Disconnected => {
                return Err(io::Error::other("every fetch worker stopped"));
            }
            FetchPoll::Empty => {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
