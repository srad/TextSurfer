use encoding_rs::{CoderResult, Decoder, Encoding};
use std::sync::Arc;
use url::Url;

use crate::html::IncrementalHtmlParser;
use crate::net::html_encoding;
use crate::script::JsEngineFactory;

use super::{PageLoad, PageLoadOptions};

pub struct PendingPageLoad {
    parser: IncrementalHtmlParser,
    body: Vec<u8>,
    offset: usize,
    decoder: Decoder,
    document_url: Url,
    html_encoding: &'static Encoding,
    options: PageLoadOptions,
    script_factory: Option<Arc<dyn JsEngineFactory>>,
}

impl PendingPageLoad {
    pub fn new(
        source: String,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        Self::from_encoded_bytes(
            source.into_bytes(),
            document_url,
            html_encoding,
            options,
            None,
        )
    }

    pub fn from_bytes(
        body: Vec<u8>,
        header_charset: Option<&str>,
        document_url: Url,
        options: PageLoadOptions,
    ) -> Self {
        let encoding = html_encoding(&body, header_charset);
        Self::from_encoded_bytes(body, document_url, encoding, options, None)
    }

    pub fn from_bytes_with_scripts(
        body: Vec<u8>,
        header_charset: Option<&str>,
        document_url: Url,
        options: PageLoadOptions,
        script_factory: Arc<dyn JsEngineFactory>,
    ) -> Self {
        let encoding = html_encoding(&body, header_charset);
        Self::from_encoded_bytes(body, document_url, encoding, options, Some(script_factory))
    }

    fn from_encoded_bytes(
        body: Vec<u8>,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
        script_factory: Option<Arc<dyn JsEngineFactory>>,
    ) -> Self {
        Self {
            parser: IncrementalHtmlParser::new(options.scripting),
            decoder: html_encoding.new_decoder_with_bom_removal(),
            body,
            offset: 0,
            document_url,
            html_encoding,
            options,
            script_factory,
        }
    }

    pub fn step(&mut self, max_bytes: usize, now: std::time::Duration) -> Option<PageLoad> {
        let end = self
            .offset
            .saturating_add(max_bytes.max(1))
            .min(self.body.len());
        let last = end == self.body.len();
        loop {
            let input = &self.body[self.offset..end];
            let capacity = self
                .decoder
                .max_utf8_buffer_length(input.len())
                .unwrap_or(input.len().saturating_mul(3).saturating_add(16))
                .max(16);
            let mut decoded = String::with_capacity(capacity);
            let (result, read, _) = self.decoder.decode_to_string(input, &mut decoded, last);
            self.offset += read;
            if !decoded.is_empty() {
                self.parser.feed(&decoded);
            }
            if result == CoderResult::InputEmpty {
                break;
            }
        }
        if self.offset < self.body.len() {
            return None;
        }
        let outcome = self.parser.finish()?;
        let mut options = self.options;
        options.started = now;
        let mut load = PageLoad::from_outcome_with_scripts(
            outcome,
            self.document_url.clone(),
            self.html_encoding,
            options,
            self.script_factory.clone(),
        );
        load.defer_rendering();
        Some(load)
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.offset, self.body.len())
    }
}
