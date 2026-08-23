use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use url::Url;

use crate::core::geom::Size;
use crate::core::style::Palette;
use crate::core::url::url_fix;
use crate::css::ColorScheme;
use crate::net::{
    Fetch, FetchPayload, FetchPool, FetchRequest, ResourceId, charset_from_content_type, decode,
    decode_text,
};
use crate::pipeline::page_load::{PageLoad, PageLoadOptions, STYLESHEET_DEADLINE};
use crate::pipeline::render::{ResponseKind, response_kind};

pub fn dump_lines(
    fetch: Arc<dyn Fetch>,
    url: &str,
    viewport: Size,
    palette: Palette,
) -> io::Result<Vec<String>> {
    let fixed = url_fix(url);
    let parsed = Url::parse(&fixed)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let pool = FetchPool::spawn(fetch, 4);
    pool.submit(0, 0, ResourceId::DOCUMENT, FetchRequest { url: parsed });
    let response = loop {
        if let Some(payload) = pool.try_recv() {
            break payload
                .result
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let charset = response
        .content_type
        .as_deref()
        .and_then(charset_from_content_type);
    let viewport = Size {
        cols: viewport.cols.max(1),
        rows: viewport.rows.max(1),
    };
    let mut detach_pool = false;
    let lines = match response_kind(response.content_type.as_deref()) {
        ResponseKind::Html => {
            let decoded = decode(&response.body, charset.as_deref());
            let mut load = PageLoad::new(
                &decoded.text,
                response.final_url,
                decoded.encoding,
                PageLoadOptions {
                    viewport,
                    palette,
                    scripting: false,
                    color_scheme: ColorScheme::Dark,
                    started: Duration::ZERO,
                },
            );
            let started = Instant::now();
            loop {
                for command in load.take_commands() {
                    pool.submit(0, 0, command.resource_id, FetchRequest { url: command.url });
                }
                if load.take_cancel_requested() {
                    pool.cancel(0, 0);
                }
                if load.applicable_is_settled() || started.elapsed() >= STYLESHEET_DEADLINE {
                    detach_pool = !load.is_settled();
                    pool.cancel(0, 0);
                    break;
                }
                if let Some(FetchPayload {
                    resource_id,
                    result,
                    ..
                }) = pool.try_recv()
                {
                    let _ = load.deliver(resource_id, result);
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            load.force_render().painted.text_lines()
        }
        ResponseKind::PlainText => decode_text(&response.body, charset.as_deref())
            .text
            .lines()
            .map(str::to_string)
            .collect(),
        ResponseKind::Unsupported(kind) => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported content type: {kind}"),
            ));
        }
    };
    if detach_pool {
        pool.shutdown_without_waiting();
    }
    Ok(lines)
}
