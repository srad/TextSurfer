use std::sync::Arc;

use super::*;
use crate::core::geom::Size;
use crate::core::style::Palette;
use crate::net::{Fetch, FetchError, FetchRequest, FetchResponse};

struct ExternalDumpFetch;

impl Fetch for ExternalDumpFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let (body, content_type) = if request.url.path().ends_with("site.css") {
            (b"#navigation { display:none }".to_vec(), "text/css")
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

#[test]
fn dump_uses_the_external_stylesheet_load_driver() {
    let lines = dump_lines(
        Arc::new(ExternalDumpFetch),
        "https://example.com/article",
        Size { cols: 80, rows: 24 },
        Palette::DEFAULT,
    )
    .unwrap();
    assert!(lines.iter().any(|line| line.contains("article")));
    assert!(!lines.iter().any(|line| line.contains("sidebar")));
}

struct RefreshDumpFetch;

impl Fetch for RefreshDumpFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let body = if request.url.path() == "/wrapper" {
            b"<meta http-equiv=refresh content='0;url=/destination'>wrapper".to_vec()
        } else {
            b"<main>destination</main>".to_vec()
        };
        Ok(FetchResponse {
            final_url: request.url.clone(),
            status: 200,
            body,
            content_type: Some("text/html; charset=utf-8".to_string()),
        })
    }
}

#[test]
fn dump_follows_an_immediate_declarative_refresh() {
    let lines = dump_lines(
        Arc::new(RefreshDumpFetch),
        "https://example.com/wrapper",
        Size { cols: 80, rows: 24 },
        Palette::DEFAULT,
    )
    .unwrap();
    assert!(lines.iter().any(|line| line.contains("destination")));
    assert!(!lines.iter().any(|line| line.contains("wrapper")));
}

struct LoopingRefreshDumpFetch;

impl Fetch for LoopingRefreshDumpFetch {
    fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        Ok(FetchResponse {
            final_url: request.url.clone(),
            status: 200,
            body: b"<meta http-equiv=refresh content='0;url=/loop'>".to_vec(),
            content_type: Some("text/html; charset=utf-8".to_string()),
        })
    }
}

#[test]
fn dump_bounds_declarative_refresh_chains() {
    let error = dump_lines(
        Arc::new(LoopingRefreshDumpFetch),
        "https://example.com/loop",
        Size { cols: 80, rows: 24 },
        Palette::DEFAULT,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "automatic redirect limit reached");
}
