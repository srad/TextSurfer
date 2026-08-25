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
